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

    let (windows_forced, mac_forced, webgl_forced, linux_forced) = if state.registry.enabled() {
        (
            state.registry.pending_jobs_for("windows").await?,
            state.registry.pending_jobs_for("mac").await?,
            state.registry.pending_jobs_for("webgl").await?,
            state.registry.pending_jobs_for("linux").await?,
        )
    } else {
        Default::default()
    };

    let mut q = QueuesStatus::default();
    let mut windows_seen: HashSet<String> = HashSet::new();
    let mut mac_seen: HashSet<String> = HashSet::new();
    let mut webgl_seen: HashSet<String> = HashSet::new();
    let mut linux_seen: HashSet<String> = HashSet::new();
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
            q.webgl_pending_jobs.push(id.clone());
        }
        if !matches!(m.linux_status(), crate::types::BuildStatus::Complete)
            || linux_forced.contains(&id)
        {
            linux_seen.insert(id.clone());
            q.linux_pending_jobs.push(id);
        }
    }

    for id in windows_forced.difference(&windows_seen) {
        q.windows_pending_jobs.push(id.clone());
    }
    for id in mac_forced.difference(&mac_seen) {
        q.mac_pending_jobs.push(id.clone());
    }
    for id in webgl_forced.difference(&webgl_seen) {
        q.webgl_pending_jobs.push(id.clone());
    }
    for id in linux_forced.difference(&linux_seen) {
        q.linux_pending_jobs.push(id.clone());
    }

    if has_valid_bearer(&state, &headers) {
        q.paused = Some(state.registry.queue_paused().await?);
    }
    Ok(Json(q))
}

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
                let enqueued = state
                    .registry
                    .enqueue_build(entity_id, "admin-bearer")
                    .await?;
                let attempts = state
                    .registry
                    .record_retry(entity_id, "admin-bearer")
                    .await?;
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

pub async fn post_queues_pause(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    set_paused(&state, &headers, true).await
}

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
    let value = state
        .registry
        .set_queue_paused(paused, "admin-bearer")
        .await?;
    Ok(Json(json!({ "ok": true, "paused": value })))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct RetryBody {
    #[serde(default, alias = "ids")]
    pub entity_ids: Vec<String>,
}
