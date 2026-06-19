use std::collections::HashSet;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::json;

use crate::http::auth::{has_valid_bearer, require_bearer};
use crate::http::errors::{ApiError, ApiResult};
use crate::types::QueuesStatus;
use crate::AppState;

const QUEUE_SCAN_LIMIT: i64 = 2000;

pub async fn get_queues_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<QueuesStatus>> {
    let ids = state
        .content
        .active_entity_ids_of_types(&["scene", "wearable", "emote"], QUEUE_SCAN_LIMIT)
        .await?;

    // Union operator-forced re-enqueues (build-job queue) with the disk-derived
    // pending set. Additive only: never removes a job the disk manifests expose.
    let (windows_forced, mac_forced, webgl_forced) = if state.registry.enabled() {
        (
            state.registry.pending_jobs_for("windows").await?,
            state.registry.pending_jobs_for("mac").await?,
            state.registry.pending_jobs_for("webgl").await?,
        )
    } else {
        Default::default()
    };

    let mut q = QueuesStatus::default();
    let mut windows_seen: HashSet<String> = HashSet::new();
    let mut mac_seen: HashSet<String> = HashSet::new();
    let mut webgl_seen: HashSet<String> = HashSet::new();
    for id in ids {
        let m = state.manifests.get(&id).await;
        if !matches!(m.windows_status(), crate::types::BuildStatus::Complete)
            || windows_forced.contains(&id)
        {
            windows_seen.insert(id.clone());
            q.windows_pending_jobs.push(id.clone());
        }
        if !matches!(m.mac_status(), crate::types::BuildStatus::Complete)
            || mac_forced.contains(&id)
        {
            mac_seen.insert(id.clone());
            q.mac_pending_jobs.push(id.clone());
        }
        if !matches!(m.webgl_status(), crate::types::BuildStatus::Complete)
            || webgl_forced.contains(&id)
        {
            webgl_seen.insert(id.clone());
            q.webgl_pending_jobs.push(id);
        }
    }
    // Forced re-enqueues outside the active scan window still need to surface.
    for id in windows_forced.difference(&windows_seen) {
        q.windows_pending_jobs.push(id.clone());
    }
    for id in mac_forced.difference(&mac_seen) {
        q.mac_pending_jobs.push(id.clone());
    }
    for id in webgl_forced.difference(&webgl_seen) {
        q.webgl_pending_jobs.push(id.clone());
    }
    // Expose the operator pause flag to authenticated admins only — keep the
    // public shape unchanged for unauthenticated/scene callers.
    if has_valid_bearer(&state, &headers) {
        q.paused = Some(state.registry.queue_paused().await?);
    }
    Ok(Json(q))
}

/// POST /queues/retry — re-trigger a build for one or more entities. This does
/// the closest REAL effect available to this crate: it resets the target
/// entity's per-platform build status to Pending and re-enqueues it into the
/// catalyrst-owned build-job queue, so (a) a build runner polling that queue
/// picks it up, and (b) `/queues/status` immediately reports the entity as
/// pending again — even for builds abgen already marked Complete on disk. It
/// also records the retry intent (attempt counter) and busts the cached
/// manifest so the next status read re-reads disk.
pub async fn post_queues_retry(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<RetryBody>>,
) -> ApiResult<Json<serde_json::Value>> {
    require_bearer(&state, &headers)?;
    if !state.registry.enabled() {
        return Err(ApiError::not_implemented(
            "queue retry requires the ab_registry DB",
        ));
    }

    let entity_ids: Vec<String> = body
        .map(|b| b.0.entity_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if entity_ids.is_empty() {
        return Err(ApiError::bad_request("No entity ids provided"));
    }

    let mut retried: Vec<serde_json::Value> = Vec::new();
    let mut failures: Vec<serde_json::Value> = Vec::new();
    for entity_id in &entity_ids {
        match state.content.resolve_one(entity_id).await {
            Ok(Some(_)) => {
                // Real re-trigger: reset every platform's status to Pending and
                // enqueue the build so a worker claims it and /queues/status
                // reports it as pending.
                let enqueued = state.registry.enqueue_build(entity_id, "admin-bearer").await?;
                let attempts = state.registry.record_retry(entity_id, "admin-bearer").await?;
                state.manifests.invalidate(entity_id).await;
                retried.push(json!({
                    "entityId": entity_id,
                    "attempts": attempts,
                    "enqueued": enqueued,
                }));
            }
            Ok(None) => {
                failures.push(json!({
                    "entityId": entity_id,
                    "error": "Entity not found in catalyst",
                }));
            }
            Err(err) => {
                tracing::error!(error = %err, entity_id = %entity_id, "retry resolve failed");
                failures.push(json!({ "entityId": entity_id, "error": err.to_string() }));
            }
        }
    }

    Ok(Json(json!({ "retried": retried, "failures": failures })))
}

/// POST /queues/pause — set the operator queue-pause flag.
pub async fn post_queues_pause(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    set_paused(&state, &headers, true).await
}

/// POST /queues/resume — clear the operator queue-pause flag.
pub async fn post_queues_resume(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    set_paused(&state, &headers, false).await
}

async fn set_paused(
    state: &AppState,
    headers: &HeaderMap,
    paused: bool,
) -> ApiResult<Json<serde_json::Value>> {
    require_bearer(state, headers)?;
    if !state.registry.enabled() {
        return Err(ApiError::not_implemented(
            "queue pause/resume requires the ab_registry DB",
        ));
    }
    let value = state.registry.set_queue_paused(paused, "admin-bearer").await?;
    Ok(Json(json!({ "ok": true, "paused": value })))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct RetryBody {
    /// Entity ids to re-enqueue. Accepts both `entityIds` and `ids`.
    #[serde(default, alias = "ids")]
    pub entity_ids: Vec<String>,
}
