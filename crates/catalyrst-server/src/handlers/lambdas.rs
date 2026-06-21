use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cache::ResponseCache;
use crate::state::AppState;

const PROFILE_CACHE_TTL: Duration = Duration::from_secs(30);
const PROFILE_CACHE_MAX_ENTRIES: usize = 50_000;
const PROFILE_BATCH_MAX: usize = 50;

fn profile_cache() -> &'static Arc<ResponseCache<String, Value>> {
    static C: OnceLock<Arc<ResponseCache<String, Value>>> = OnceLock::new();
    C.get_or_init(|| {
        Arc::new(ResponseCache::new(
            "profile",
            PROFILE_CACHE_TTL,
            PROFILE_CACHE_MAX_ENTRIES,
        ))
    })
}

fn profiles_batch_cache() -> &'static Arc<ResponseCache<String, Value>> {
    static C: OnceLock<Arc<ResponseCache<String, Value>>> = OnceLock::new();
    C.get_or_init(|| {
        Arc::new(ResponseCache::new(
            "profiles_batch",
            PROFILE_CACHE_TTL,
            PROFILE_CACHE_MAX_ENTRIES,
        ))
    })
}

/// `default`, `default0`..`defaultN` are not deployed profiles — the classic
/// catalyst lambdas synthesizes a built-in starter avatar for them. We mirror
/// that so tools requesting `default` (e.g. wearable-preview) get a usable base
/// avatar instead of a 404.
fn is_default_profile_id(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    id == "default"
        || id
            .strip_prefix("default")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn synthetic_default_profile(id: &str) -> Value {
    json!({
        "avatars": [{
            "userId": id,
            "name": id,
            "hasClaimedName": false,
            "ethAddress": id,
            "version": 0,
            "tutorialStep": 0,
            "avatar": {
                "bodyShape": "urn:decentraland:off-chain:base-avatars:BaseMale",
                "wearables": [
                    "urn:decentraland:off-chain:base-avatars:eyebrows_00",
                    "urn:decentraland:off-chain:base-avatars:mouth_00",
                    "urn:decentraland:off-chain:base-avatars:casual_hair_01",
                    "urn:decentraland:off-chain:base-avatars:green_hoodie",
                    "urn:decentraland:off-chain:base-avatars:brown_pants",
                    "urn:decentraland:off-chain:base-avatars:sneakers"
                ],
                "emotes": [],
                "snapshots": { "face256": "", "body": "" },
                "eyes": { "color": { "r": 0.37, "g": 0.22, "b": 0.19 } },
                "hair": { "color": { "r": 0.6, "g": 0.46, "b": 0.27 } },
                "skin": { "color": { "r": 0.94, "g": 0.76, "b": 0.6 } }
            }
        }],
        "timestamp": 0
    })
}

async fn fetch_profile_for_id(state: &AppState, id: &str) -> Option<Value> {
    if is_default_profile_id(id) {
        return Some(synthetic_default_profile(id));
    }
    let key = id.to_lowercase();
    let state_arc = state;
    let cached = profile_cache()
        .get_or_fetch(key.clone(), || async move {
            let entities = state_arc
                .database
                .active_entities_by_pointers(std::slice::from_ref(&key))
                .await
                .unwrap_or_default();
            let Some(entity) = entities.into_iter().next() else {
                return Ok::<Value, ()>(Value::Null);
            };
            let squid_pool = state_arc.squid_pool.as_ref();
            let cdn_base = &state_arc.profile_cdn_base_url;
            let processed =
                super::profile_processing::process_profile(&entity, squid_pool, cdn_base)
                    .await
                    .unwrap_or(Value::Null);
            Ok::<Value, ()>(processed)
        })
        .await
        .ok()?;
    if cached.is_null() {
        None
    } else {
        Some(cached)
    }
}

#[derive(Debug, Deserialize)]
pub struct ProfilesRequest {
    #[serde(default)]
    pub ids: Option<Vec<String>>,
}

pub async fn profiles(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ProfilesRequest>,
) -> Response {
    let ids = match body.ids {
        Some(ids) => ids,
        None => {
            return crate::errors::bad_request(
                "No profile ids were specified. Expected ids:string[] in body",
            )
        }
    };

    if ids.is_empty() {
        return Json(Value::Array(vec![])).into_response();
    }

    let modified_since: Option<i64> = headers
        .get("if-modified-since")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            chrono::DateTime::parse_from_rfc2822(s)
                .ok()
                .map(|dt| dt.timestamp_millis())
        });

    let pointers: Vec<String> = ids.iter().map(|id| id.to_lowercase()).collect();

    let cache_eligible = modified_since.is_none() && pointers.len() <= PROFILE_BATCH_MAX;

    if cache_eligible {
        let mut sorted = pointers.clone();
        sorted.sort();
        sorted.dedup();
        let cache_key = sorted.join(",");
        let state_arc = state.clone();
        let pointers_for_fetch = pointers.clone();
        let cached: Result<Value, ()> = profiles_batch_cache()
            .get_or_fetch(cache_key, move || async move {
                let entities = state_arc
                    .database
                    .active_entities_by_pointers(&pointers_for_fetch)
                    .await
                    .map_err(|_| ())?;
                let squid_pool = state_arc.squid_pool.as_ref();
                let cdn_base = &state_arc.profile_cdn_base_url;
                let mut profiles: Vec<Value> = Vec::with_capacity(entities.len());
                for entity in &entities {
                    if let Some(processed) =
                        super::profile_processing::process_profile(entity, squid_pool, cdn_base)
                            .await
                    {
                        profiles.push(processed);
                    }
                }
                Ok::<Value, ()>(Value::Array(profiles))
            })
            .await;
        return match cached {
            Ok(v) => Json(v).into_response(),
            Err(_) => crate::errors::internal_server_error(),
        };
    }

    let entities = match state.database.active_entities_by_pointers(&pointers).await {
        Ok(e) => e,
        Err(_) => return crate::errors::internal_server_error(),
    };

    if let Some(threshold) = modified_since {
        let all_stale = entities.iter().all(|entity| {
            entity
                .get("timestamp")
                .and_then(|t| t.as_f64())
                .map(|ts| (ts as i64) <= threshold)
                .unwrap_or(false)
        });
        if all_stale && !entities.is_empty() {
            return StatusCode::NOT_MODIFIED.into_response();
        }
    }

    let squid_pool = state.squid_pool.as_ref();
    let cdn_base = &state.profile_cdn_base_url;
    let mut profiles: Vec<Value> = Vec::with_capacity(entities.len());
    for entity in &entities {
        if let Some(processed) =
            super::profile_processing::process_profile(entity, squid_pool, cdn_base).await
        {
            profiles.push(processed);
        }
    }

    Json(Value::Array(profiles)).into_response()
}

pub async fn profile_by_id(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match fetch_profile_for_id(&state, &id).await {
        Some(metadata) => Json(metadata).into_response(),
        None => crate::errors::not_found("Profile not found"),
    }
}

pub async fn profile_alias(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match fetch_profile_for_id(&state, &id).await {
        Some(processed) => Json(processed),
        None => Json(json!({ "avatars": [], "timestamp": 0 })),
    }
}

async fn items_by_owner(
    state: &AppState,
    owner: &str,
    category: &str,
    include_definitions: bool,
    extract: impl Fn(&Value, &str) -> Option<Value>,
) -> Response {
    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => return Json(json!([])).into_response(),
    };
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT replace(urn, ':mainnet:', ':ethereum:') AS urn, count(*) \
         FROM squid_marketplace.nft \
         WHERE category = $1 AND urn IS NOT NULL AND owner_address = lower($2) \
         GROUP BY replace(urn, ':mainnet:', ':ethereum:') ORDER BY 1",
    )
    .bind(category)
    .bind(owner)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if !include_definitions {
        let body: Vec<Value> = rows
            .into_iter()
            .map(|(urn, amount)| json!({ "urn": urn, "amount": amount }))
            .collect();
        return Json(json!(body)).into_response();
    }

    let pointers: Vec<String> = rows.iter().map(|(urn, _)| urn.to_lowercase()).collect();
    let entities = if pointers.is_empty() {
        Vec::new()
    } else {
        state
            .database
            .active_entities_by_pointers(&pointers)
            .await
            .unwrap_or_default()
    };

    let content_public_url = &state.content_public_url;
    let mut defs_by_id: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for e in &entities {
        if let Some(def) = extract(e, content_public_url) {
            if let Some(id) = def.get("id").and_then(|v| v.as_str()) {
                defs_by_id.insert(id.to_lowercase(), def);
            }
        }
    }

    let body: Vec<Value> = rows
        .into_iter()
        .map(|(urn, amount)| {
            let mut obj = json!({ "urn": urn, "amount": amount });
            if let Some(def) = defs_by_id.get(&urn.to_lowercase()) {
                obj["definition"] = def.clone();
            }
            obj
        })
        .collect();
    Json(json!(body)).into_response()
}

fn has_include_definitions(req: &Request) -> bool {
    req.uri()
        .query()
        .unwrap_or("")
        .split('&')
        .any(|p| p == "includeDefinitions" || p.starts_with("includeDefinitions="))
}

pub async fn wearables_by_owner(
    State(state): State<Arc<AppState>>,
    Path(owner): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let include = has_include_definitions(&req);
    items_by_owner(
        &state,
        &owner,
        "wearable",
        include,
        super::definitions::extract_wearable_definition,
    )
    .await
}

pub async fn emotes_by_owner(
    State(state): State<Arc<AppState>>,
    Path(owner): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let include = has_include_definitions(&req);
    items_by_owner(
        &state,
        &owner,
        "emote",
        include,
        super::definitions::extract_emote_definition,
    )
    .await
}

pub async fn collections_wearables() -> impl IntoResponse {
    Json(json!({
        "wearables": [],
        "filters": {},
        "pagination": {
            "limit": 100,
            "lastId": null,
            "next": null
        }
    }))
}

pub async fn collections_emotes() -> impl IntoResponse {
    Json(json!({
        "emotes": [],
        "filters": {},
        "pagination": {
            "limit": 100,
            "lastId": null,
            "next": null
        }
    }))
}

pub async fn explore_realms(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let realm_name = state.realm_name.as_deref().unwrap_or("catalyrst");

    Json(json!([
        {
            "serverName": realm_name,
            "url": state.content_public_url,
            "usersCount": 0,
            "maxUsers": null
        }
    ]))
}

pub async fn lambdas_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let current_time = chrono::Utc::now().timestamp_millis();

    Json(json!({
        "version": state.lambdas_version,
        "currentTime": current_time,
        "commitHash": state.commit_hash
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::ResponseCache;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn profile_cache_second_call_is_a_hit() {
        let cache: ResponseCache<String, Value> =
            ResponseCache::new("profile_test", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));
        let make_profile = |addr: String, c: Arc<AtomicUsize>| async move {
            c.fetch_add(1, Ordering::SeqCst);

            Ok::<_, ()>(json!({ "id": addr, "avatars": [] }))
        };

        let c = counter.clone();
        let v1 = cache
            .get_or_fetch("0xabc".to_string(), || make_profile("0xabc".to_string(), c))
            .await
            .unwrap();
        let c = counter.clone();
        let v2 = cache
            .get_or_fetch("0xabc".to_string(), || make_profile("0xabc".to_string(), c))
            .await
            .unwrap();
        assert_eq!(v1, v2);
        assert_eq!(counter.load(Ordering::SeqCst), 1, "second call must HIT");
    }

    #[tokio::test]
    async fn profiles_batch_cache_keys_normalize_order() {
        let cache: ResponseCache<String, Value> =
            ResponseCache::new("profiles_batch_test", Duration::from_secs(60), 100);
        let counter = Arc::new(AtomicUsize::new(0));

        let key_a = {
            let mut v = vec!["0xa".to_string(), "0xb".to_string()];
            v.sort();
            v.dedup();
            v.join(",")
        };
        let key_b = {
            let mut v = vec!["0xb".to_string(), "0xa".to_string()];
            v.sort();
            v.dedup();
            v.join(",")
        };
        assert_eq!(key_a, key_b);

        let c = counter.clone();
        cache
            .get_or_fetch(key_a.clone(), || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!([{ "id": "0xa" }, { "id": "0xb" }]))
            })
            .await
            .unwrap();
        let c = counter.clone();
        cache
            .get_or_fetch(key_b, || async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(json!([]))
            })
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
