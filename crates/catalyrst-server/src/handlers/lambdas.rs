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
const PROFILE_IDS_MAX: usize = 1000;
/// Max profiles whose squid ownership/ENS lookups run concurrently. `buffered`
/// (not `buffer_unordered`) keeps output order identical to the input entity
/// order, matching the existing `lambdas_contracts.rs` pattern.
const PROFILE_PROCESS_CONCURRENCY: usize = 8;

/// Process a batch of profile entities concurrently (bounded by
/// `PROFILE_PROCESS_CONCURRENCY`), preserving input order and dropping entities
/// that yield no profile. Takes the entities by value and moves each into its
/// own `async move`: a closure that borrowed the entity (`FnMut(&Value) ->
/// Future`) trips a higher-ranked-lifetime limitation ("FnOnce is not general
/// enough") when the resulting future has to be `Send` inside the `get_or_fetch`
/// cache closure. Moving owned values sidesteps it with no extra clones.
async fn process_profiles_concurrent(
    entities: Vec<Value>,
    squid_pool: Option<&sqlx::PgPool>,
    cdn_base: &str,
) -> Vec<Value> {
    use futures::stream::StreamExt;
    futures::stream::iter(entities.into_iter().map(|entity| async move {
        super::profile_processing::process_profile(&entity, squid_pool, cdn_base).await
    }))
    .buffered(PROFILE_PROCESS_CONCURRENCY)
    .filter_map(|p| async move { p })
    .collect()
    .await
}

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

const MISSING_PROFILE_IDS_MSG: &str =
    "No profile ids were specified. Expected ids:string[] in body";

#[derive(Debug, PartialEq, Eq)]
enum ProfileIdsCheck {
    MissingOrEmpty,
    Valid,
    TooMany,
}

fn validate_profile_ids(ids: Option<&[String]>) -> ProfileIdsCheck {
    match ids {
        None => ProfileIdsCheck::MissingOrEmpty,
        Some([]) => ProfileIdsCheck::MissingOrEmpty,
        Some(ids) if ids.len() > PROFILE_IDS_MAX => ProfileIdsCheck::TooMany,
        Some(_) => ProfileIdsCheck::Valid,
    }
}

fn early_profiles_response(check: ProfileIdsCheck) -> Option<Response> {
    match check {
        ProfileIdsCheck::MissingOrEmpty => {
            Some(crate::errors::bad_request(MISSING_PROFILE_IDS_MSG))
        }
        ProfileIdsCheck::TooMany => Some(crate::errors::bad_request(&format!(
            "Too many profile ids were specified. Maximum allowed is {}",
            PROFILE_IDS_MAX
        ))),
        ProfileIdsCheck::Valid => None,
    }
}

pub async fn profiles(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    crate::extractors::JsonBody(body): crate::extractors::JsonBody<ProfilesRequest>,
) -> Response {
    if let Some(resp) = early_profiles_response(validate_profile_ids(body.ids.as_deref())) {
        return resp;
    }
    let ids = body
        .ids
        .expect("validate_profile_ids guarantees a non-empty id list");

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
                let profiles =
                    process_profiles_concurrent(entities, squid_pool, cdn_base).await;
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
    let profiles = process_profiles_concurrent(entities, squid_pool, cdn_base).await;

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

fn third_party_collection_name(collection_id: &str) -> Option<String> {
    let cleaned: String = collection_id
        .split(':')
        .take(5)
        .collect::<Vec<_>>()
        .join(":");
    if !super::lambdas_user_items::is_third_party_name_urn(&cleaned) {
        return None;
    }
    cleaned.split(':').nth(4).map(|s| s.to_string())
}

async fn third_party_items_by_owner(
    state: &AppState,
    owner: &str,
    collection_id: &str,
    include_definitions: bool,
    extract: impl Fn(&Value, &str) -> Option<Value>,
) -> Response {
    let name = match third_party_collection_name(collection_id) {
        Some(name) => name,
        None => {
            return crate::errors::bad_request(
                "'collectionId' must be a valid third-party collection URN",
            );
        }
    };

    let elements =
        super::lambdas_user_items::fetch_all_third_party_wearables(state, owner, Some(&name)).await;

    let content_public_url = &state.content_public_url;
    let body: Vec<Value> = elements
        .iter()
        .map(|e| {
            let mut obj = json!({ "urn": e.urn, "amount": e.individual_data.len() });
            if include_definitions {
                if let Some(def) = extract(&e.entity, content_public_url) {
                    obj["definition"] = def;
                }
            }
            obj
        })
        .collect();
    Json(json!(body)).into_response()
}

async fn items_by_owner(
    state: &AppState,
    owner: &str,
    category: &str,
    include_definitions: bool,
    collection_id: Option<&str>,
    extract: impl Fn(&Value, &str) -> Option<Value>,
) -> Response {
    if let Some(collection_id) = collection_id {
        return third_party_items_by_owner(
            state,
            owner,
            collection_id,
            include_definitions,
            extract,
        )
        .await;
    }

    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => return Json(json!([])).into_response(),
    };

    let sql = if super::lease_overlay::usage_grants_present(pool).await {
        "SELECT urn, count(*) AS amount, COALESCE(max(rarity), '') AS rarity FROM ( \
             SELECT replace(n.urn, ':mainnet:', ':ethereum:') AS urn, i.rarity AS rarity \
             FROM squid_marketplace.nft n \
             LEFT JOIN squid_marketplace.item i ON n.item_id = i.id \
             WHERE n.category = $1 AND n.urn IS NOT NULL AND n.owner_address = lower($2) \
           UNION ALL \
             SELECT replace(ug.urn, ':mainnet:', ':ethereum:') AS urn, NULL::text AS rarity \
             FROM marketplace.usage_grants ug \
             WHERE ug.status = 'active' AND ug.category = $1 \
               AND ug.urn IS NOT NULL AND ug.grantee_address = lower($2) \
         ) owned \
         GROUP BY urn"
    } else {
        "SELECT replace(n.urn, ':mainnet:', ':ethereum:') AS urn, count(*) AS amount, \
                COALESCE(max(i.rarity), '') AS rarity \
         FROM squid_marketplace.nft n \
         LEFT JOIN squid_marketplace.item i ON n.item_id = i.id \
         WHERE n.category = $1 AND n.urn IS NOT NULL AND n.owner_address = lower($2) \
         GROUP BY 1"
    };
    let mut rows: Vec<(String, i64, String)> = sqlx::query_as(sql)
        .bind(category)
        .bind(owner)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    sort_owned_by_rarity_then_urn(&mut rows);

    if !include_definitions {
        let body: Vec<Value> = rows
            .into_iter()
            .map(|(urn, amount, _)| json!({ "urn": urn, "amount": amount }))
            .collect();
        return Json(json!(body)).into_response();
    }

    let pointers: Vec<String> = rows.iter().map(|(urn, ..)| urn.to_lowercase()).collect();
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
        .map(|(urn, amount, _)| {
            let mut obj = json!({ "urn": urn, "amount": amount });
            if let Some(def) = defs_by_id.get(&urn.to_lowercase()) {
                obj["definition"] = def.clone();
            }
            obj
        })
        .collect();
    Json(json!(body)).into_response()
}

/// Sort grouped (urn, amount, rarity) rows the way the reference catalyst
/// orders by-owner lists: rarity rank descending (unique > mythic > ... >
/// common, unknown last), then URN ascending. Mirrors the default
/// `orderBy=rarity DESC` sort of the users/<addr>/{wearables,emotes} handlers
/// in `lambdas_user_items::validate_and_sort`.
fn sort_owned_by_rarity_then_urn(rows: &mut [(String, i64, String)]) {
    use super::definitions::{locale_cmp, rarity_rank};
    rows.sort_by(|a, b| {
        rarity_rank(&b.2)
            .cmp(&rarity_rank(&a.2))
            .then_with(|| locale_cmp(&a.0, &b.0))
    });
}

fn has_include_definitions(req: &Request) -> bool {
    req.uri()
        .query()
        .unwrap_or("")
        .split('&')
        .any(|p| p == "includeDefinitions" || p.starts_with("includeDefinitions="))
}

fn collection_id_param(req: &Request) -> Option<String> {
    let params = crate::query_params::parse_query_string(req.uri().query().unwrap_or(""));
    crate::query_params::qs_get_string(&params, "collectionId").filter(|s| !s.is_empty())
}

pub async fn wearables_by_owner(
    State(state): State<Arc<AppState>>,
    Path(owner): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let include = has_include_definitions(&req);
    let collection_id = collection_id_param(&req);
    items_by_owner(
        &state,
        &owner,
        "wearable",
        include,
        collection_id.as_deref(),
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
    let collection_id = collection_id_param(&req);
    items_by_owner(
        &state,
        &owner,
        "emote",
        include,
        collection_id.as_deref(),
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

    #[test]
    fn by_owner_rows_sort_rarity_desc_then_urn_asc() {
        let row = |urn: &str, rarity: &str| (urn.to_string(), 1_i64, rarity.to_string());
        let mut rows = vec![
            row("urn:z:common", "common"),
            row("urn:b:mythic", "mythic"),
            row("urn:a:unknown", ""),
            row("urn:a:mythic", "mythic"),
            row("urn:a:unique", "unique"),
            row("urn:a:common", "common"),
        ];
        sort_owned_by_rarity_then_urn(&mut rows);
        let urns: Vec<&str> = rows.iter().map(|(urn, ..)| urn.as_str()).collect();
        assert_eq!(
            urns,
            vec![
                "urn:a:unique",
                "urn:a:mythic",
                "urn:b:mythic",
                "urn:a:common",
                "urn:z:common",
                "urn:a:unknown",
            ],
            "rarity rank DESC (unknown last), then URN ASC"
        );
    }

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

    #[test]
    fn validate_profile_ids_rejects_missing() {
        assert_eq!(validate_profile_ids(None), ProfileIdsCheck::MissingOrEmpty);
    }

    #[test]
    fn validate_profile_ids_empty_array_is_rejected_like_missing() {
        let empty: Vec<String> = Vec::new();
        assert_eq!(
            validate_profile_ids(Some(&empty)),
            ProfileIdsCheck::MissingOrEmpty
        );
    }

    #[test]
    fn validate_profile_ids_accepts_single() {
        let ids = vec!["0xabc".to_string()];
        assert_eq!(validate_profile_ids(Some(&ids)), ProfileIdsCheck::Valid);
    }

    #[test]
    fn validate_profile_ids_accepts_exactly_1000() {
        let ids: Vec<String> = (0..PROFILE_IDS_MAX).map(|i| format!("0x{i}")).collect();
        assert_eq!(ids.len(), 1000);
        assert_eq!(validate_profile_ids(Some(&ids)), ProfileIdsCheck::Valid);
    }

    #[test]
    fn validate_profile_ids_rejects_over_1000() {
        let ids: Vec<String> = (0..=PROFILE_IDS_MAX).map(|i| format!("0x{i}")).collect();
        assert_eq!(ids.len(), 1001);
        assert_eq!(validate_profile_ids(Some(&ids)), ProfileIdsCheck::TooMany);
    }

    #[test]
    fn validate_profile_ids_batch_max_is_only_cache_eligibility() {
        const { assert!(PROFILE_BATCH_MAX < PROFILE_IDS_MAX) };
        let ids: Vec<String> = (0..=PROFILE_BATCH_MAX).map(|i| format!("0x{i}")).collect();
        assert!(ids.len() > PROFILE_BATCH_MAX);
        assert_eq!(validate_profile_ids(Some(&ids)), ProfileIdsCheck::Valid);
    }

    #[tokio::test]
    async fn early_profiles_response_empty_array_is_400() {
        let resp = early_profiles_response(validate_profile_ids(Some(&[])))
            .expect("empty ids must produce a terminal response");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn early_profiles_response_missing_field_is_400() {
        let resp = early_profiles_response(validate_profile_ids(None))
            .expect("missing ids must produce a terminal response");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn early_profiles_response_valid_ids_proceeds() {
        let ids = vec!["0xabc".to_string()];
        assert!(early_profiles_response(validate_profile_ids(Some(&ids))).is_none());
    }

    #[test]
    fn third_party_collection_name_accepts_valid_third_party_name_urn() {
        assert_eq!(
            third_party_collection_name(
                "urn:decentraland:matic:collections-thirdparty:cryptomotors"
            ),
            Some("cryptomotors".to_string())
        );
        assert_eq!(
            third_party_collection_name(
                "urn:decentraland:matic:collections-thirdparty:cryptomotors:car:1"
            ),
            Some("cryptomotors".to_string())
        );
    }

    #[test]
    fn third_party_collection_name_rejects_non_third_party_urns() {
        assert_eq!(
            third_party_collection_name("urn:decentraland:matic:collections-v2:0xabc:0"),
            None
        );
        assert_eq!(
            third_party_collection_name("urn:decentraland:off-chain:base-avatars:eyebrows_00"),
            None
        );
        assert_eq!(third_party_collection_name("not-a-urn"), None);
        assert_eq!(
            third_party_collection_name("urn:decentraland:matic:collections-thirdparty:"),
            None
        );
    }

    #[test]
    fn collection_id_param_absent_and_empty_are_none() {
        let req = |q: &str| {
            axum::http::Request::builder()
                .uri(format!("http://x/collections/wearables-by-owner/0xabc?{q}"))
                .body(axum::body::Body::empty())
                .unwrap()
        };
        assert_eq!(collection_id_param(&req("includeDefinitions")), None);
        assert_eq!(collection_id_param(&req("collectionId=")), None);
        assert_eq!(
            collection_id_param(&req(
                "collectionId=urn:decentraland:matic:collections-thirdparty:cryptomotors"
            )),
            Some("urn:decentraland:matic:collections-thirdparty:cryptomotors".to_string())
        );
    }
}
