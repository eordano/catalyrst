use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::{Multipart, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::{Bytes, BytesMut};
use serde_json::{json, Value};

use crate::AppState;

const MAX_DEPLOY_FILES: usize = 1000;
const MAX_AUTH_CHAIN_LENGTH: usize = 10;
const MAX_DEPLOY_FILE_BYTES: usize = 50 * 1024 * 1024;

pub const MAX_UPLOAD_SIZE_BYTES: usize = 350 * 1024 * 1024;

pub const DEFAULT_MAX_IN_FLIGHT_UPLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;

static IN_FLIGHT_UPLOAD_BYTES: AtomicU64 = AtomicU64::new(0);

struct InFlightGuard(u64);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        IN_FLIGHT_UPLOAD_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

fn declared_length_exceeds_limit(declared_len: u64) -> bool {
    declared_len > MAX_UPLOAD_SIZE_BYTES as u64
}

fn try_reserve_in_flight(reserved: u64, max: u64) -> Option<InFlightGuard> {
    let mut current = IN_FLIGHT_UPLOAD_BYTES.load(Ordering::Acquire);
    loop {
        if current > 0 && current.saturating_add(reserved) > max {
            return None;
        }
        match IN_FLIGHT_UPLOAD_BYTES.compare_exchange_weak(
            current,
            current.saturating_add(reserved),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Some(InFlightGuard(reserved)),
            Err(actual) => current = actual,
        }
    }
}

const MAX_WORLD_SIZE_BYTES: i64 = 300 * 1024 * 1024;

const ENTITY_TTL_MS: i64 = 300_000;

const DCL_ETH_SUFFIX: &str = ".dcl.eth";

fn err_response(messages: Vec<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "errors": messages })))
}

fn err_one(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    err_response(vec![message.into()])
}

fn forbidden(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "errors": [message.into()] })),
    )
}

fn internal(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "errors": [message.into()] })),
    )
}

fn present_truthy(v: &Value, key: &str) -> bool {
    match v.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => true,
    }
}

async fn store_blob(dir: &std::path::Path, hash: &str, bytes: &[u8]) -> std::io::Result<()> {
    let dst = dir.join(hash);
    if tokio::fs::try_exists(&dst).await.unwrap_or(false) {
        return Ok(());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{hash}.{}.{nonce}.part", std::process::id()));
    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, &dst).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

async fn store_auth_file(
    dir: &std::path::Path,
    entity_id: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let dst = dir.join(format!("{entity_id}.auth"));
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(
        ".{entity_id}.auth.{}.{nonce}.part",
        std::process::id()
    ));
    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, &dst).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

fn extract_auth_chain_from_fields(fields: &BTreeMap<String, String>) -> Result<Value, String> {
    if let Some(chain_str) = fields.get("authChain") {
        let chain: Value =
            serde_json::from_str(chain_str).map_err(|_| "Invalid auth chain".to_string())?;
        let arr = chain
            .as_array()
            .ok_or_else(|| "Invalid auth chain".to_string())?;
        if arr.len() > MAX_AUTH_CHAIN_LENGTH {
            return Err(format!(
                "Auth chain is too long; the maximum allowed is {MAX_AUTH_CHAIN_LENGTH} elements"
            ));
        }
        return Ok(chain);
    }

    let mut biggest_index: i64 = -1;
    let re_prefix = "authChain[";
    for key in fields.keys() {
        if let Some(rest) = key.strip_prefix(re_prefix) {
            if let Some(idx_str) = rest.split(']').next() {
                if let Ok(idx) = idx_str.parse::<i64>() {
                    if idx > biggest_index {
                        biggest_index = idx;
                    }
                }
            }
        }
    }

    if biggest_index == -1 {
        return Err("No auth chain can be derived".to_string());
    }
    if biggest_index >= MAX_AUTH_CHAIN_LENGTH as i64 {
        return Err(format!(
            "Auth chain is too long; the maximum allowed is {MAX_AUTH_CHAIN_LENGTH} elements"
        ));
    }

    let mut chain = Vec::new();
    for i in 0..=biggest_index {
        let payload = fields
            .get(&format!("authChain[{i}][payload]"))
            .ok_or_else(|| format!("Missing auth chain element at index {i}"))?;
        let signature = fields
            .get(&format!("authChain[{i}][signature]"))
            .ok_or_else(|| format!("Missing auth chain element at index {i}"))?;
        let link_type = fields
            .get(&format!("authChain[{i}][type]"))
            .ok_or_else(|| format!("Missing auth chain element at index {i}"))?;
        chain.push(json!({
            "type": link_type,
            "payload": payload,
            "signature": signature,
        }));
    }
    Ok(Value::Array(chain))
}

const MIN_PARCEL_COORDINATE: i64 = -150;
const MAX_PARCEL_COORDINATE: i64 = 150;

pub(crate) fn canon_pointer(s: &str) -> String {
    match parse_parcel_ints(s) {
        Some((x, y)) => format!("{x},{y}"),
        None => s.to_string(),
    }
}

fn parse_parcel_ints(s: &str) -> Option<(i64, i64)> {
    let (a, b) = s.split_once(',')?;
    Some((parse_signed_int(a)?, parse_signed_int(b)?))
}

fn parse_signed_int(part: &str) -> Option<i64> {
    let t = part.trim();
    let digits = t.strip_prefix('-').unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse::<i64>().ok()
}

fn canon_pointer_set(values: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = values
        .iter()
        .filter_map(|v| v.as_str().map(canon_pointer))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn validate_parcel_in_bounds(parcel: &str) -> Result<(), String> {
    let (x, y) = match parse_parcel_ints(parcel) {
        Some(xy) => xy,
        None => return Err(format!("Invalid coordinate format: {parcel}")),
    };
    if !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&x) {
        return Err(format!(
            "Coordinate X value {x} is out of bounds. Must be between {MIN_PARCEL_COORDINATE} and {MAX_PARCEL_COORDINATE}."
        ));
    }
    if !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&y) {
        return Err(format!(
            "Coordinate Y value {y} is out of bounds. Must be between {MIN_PARCEL_COORDINATE} and {MAX_PARCEL_COORDINATE}."
        ));
    }
    Ok(())
}

pub async fn deploy_entity(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    let declared: Option<u64> = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    if let Some(len) = declared {
        if declared_length_exceeds_limit(len) {
            return err_one("The multipart request is too large.").into_response();
        }
    }

    let reserved = declared
        .map(|l| l.min(MAX_UPLOAD_SIZE_BYTES as u64))
        .unwrap_or(MAX_UPLOAD_SIZE_BYTES as u64);
    let _guard = match try_reserve_in_flight(reserved, state.cfg.max_in_flight_upload_bytes) {
        Some(g) => g,
        None => {
            tracing::warn!(
                reserved,
                in_flight = IN_FLIGHT_UPLOAD_BYTES.load(Ordering::Acquire),
                max = state.cfg.max_in_flight_upload_bytes,
                "POST /entities shed: aggregate in-flight upload budget exceeded"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Retry-After", "5")],
                Json(json!({
                    "error": "Service Unavailable",
                    "message": "Server is buffering too many uploads, please retry shortly."
                })),
            )
                .into_response();
        }
    };

    deploy_entity_inner(state, headers, multipart)
        .await
        .into_response()
}

async fn deploy_entity_inner(
    state: AppState,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut files: Vec<Bytes> = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return err_one(format!("Failed to read multipart field: {e}"));
            }
        };
        let mut field = field;
        let name = field.name().unwrap_or("").to_string();

        if field.file_name().is_some() {
            if files.len() >= MAX_DEPLOY_FILES {
                return err_one(format!(
                    "deployment exceeds maximum of {MAX_DEPLOY_FILES} files"
                ));
            }
            let mut buf = BytesMut::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if buf.len().saturating_add(chunk.len()) > MAX_DEPLOY_FILE_BYTES {
                            return err_one(format!(
                                "an uploaded file exceeds {MAX_DEPLOY_FILE_BYTES} bytes"
                            ));
                        }
                        buf.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => return err_one(format!("Failed to read file data: {e}")),
                }
            }
            files.push(buf.freeze());
        } else {
            match field.text().await {
                Ok(value) => {
                    fields.insert(name, value);
                }
                Err(e) => return err_one(format!("Failed to read field value: {e}")),
            }
        }
    }

    let entity_id = match fields.get("entityId") {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return err_one("Missing entityId field"),
    };

    let auth_chain_value = match extract_auth_chain_from_fields(&fields) {
        Ok(v) => v,
        Err(e) => return err_one(e),
    };

    let mut by_hash: HashMap<String, Bytes> = HashMap::new();
    for blob in &files {
        let hash = catalyrst_hashing::hash_bytes_v1(blob);
        by_hash.entry(hash).or_insert_with(|| blob.clone());
    }

    let entity_bytes = match by_hash.get(&entity_id) {
        Some(b) => b.clone(),
        None => {
            return err_one(format!(
                "The entity file was not uploaded, or its hash does not match the entityId ({entity_id})"
            ));
        }
    };

    let entity: Value = match serde_json::from_slice(&entity_bytes) {
        Ok(v) => v,
        Err(e) => return err_one(format!("The entity file is not valid JSON: {e}")),
    };

    let mut errors: Vec<String> = Vec::new();

    let entity_type = entity.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if entity_type != "scene" {
        errors.push(format!(
            "Only scene entities can be deployed to a World (got type \"{entity_type}\")"
        ));
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    match entity.get("timestamp").and_then(|v| v.as_i64()) {
        Some(ts) => {
            if now_ms.saturating_sub(ts) > ENTITY_TTL_MS {
                errors.push(format!(
                    "The request is not authorized to deploy: the entity timestamp is too old \
                     (older than {}s)",
                    ENTITY_TTL_MS / 1000
                ));
            }
        }
        None => errors.push("The entity is missing a valid timestamp".to_string()),
    }

    let raw_world_name = entity
        .get("metadata")
        .and_then(|m| m.get("worldConfiguration"))
        .and_then(|w| w.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let mut normalized_world_name: Option<String> = None;
    let mut ownership_label: Option<String> = None;
    match raw_world_name {
        None => errors.push(
            "The metadata.worldConfiguration.name is required to deploy a scene to a World"
                .to_string(),
        ),
        Some(name) => {
            let lower = name.to_lowercase();
            if !lower.ends_with(DCL_ETH_SUFFIX) {
                errors.push(format!(
                    "Only .dcl.eth world names are supported for publishing (got \"{name}\")"
                ));
            } else {
                ownership_label = Some(lower.trim_end_matches(DCL_ETH_SUFFIX).to_string());
                normalized_world_name = Some(lower);
            }
        }
    }

    if let Some(name) = raw_world_name {
        if !state.name_denylist.check_name_deny_list(name).await {
            errors.push(format!(
                "Deployment failed: World \"{name}\" can not be deployed because the name is in the name deny list managed by Decentraland DAO."
            ));
        }
    }

    if let Some(wc) = entity
        .get("metadata")
        .and_then(|m| m.get("worldConfiguration"))
    {
        if present_truthy(wc, "dclName") {
            errors.push(
                "`dclName` in scene.json was renamed to `name`. Please update your scene.json accordingly."
                    .to_string(),
            );
        }
        if present_truthy(wc, "minimapVisible") {
            errors.push(
                "`minimapVisible` in scene.json is deprecated in favor of `{ miniMapConfig: { visible } }`. Please update your scene.json accordingly."
                    .to_string(),
            );
        }
        if present_truthy(wc, "skybox") {
            errors.push(
                "`skybox` in scene.json is deprecated in favor of `{ \"skyboxConfig\": { \"fixedTime\": 36000 }}`. Please update your scene.json accordingly."
                    .to_string(),
            );
        }
    }

    let pointers = entity
        .get("pointers")
        .and_then(|v| v.as_array())
        .map(|a| canon_pointer_set(a))
        .unwrap_or_default();
    let scene_parcels = entity
        .get("metadata")
        .and_then(|m| m.get("scene"))
        .and_then(|s| s.get("parcels"))
        .and_then(|v| v.as_array())
        .map(|a| canon_pointer_set(a))
        .unwrap_or_default();
    if pointers.is_empty() {
        errors.push("The entity has no pointers".to_string());
    } else if pointers != scene_parcels {
        errors.push("The entity pointers do not match metadata.scene.parcels".to_string());
    }

    for parcel in &pointers {
        if let Err(msg) = validate_parcel_in_bounds(parcel) {
            errors.push(msg);
        }
    }

    let mut total_content_size: i64 = 0;
    match entity.get("content") {
        Some(Value::Array(items)) => {
            for item in items {
                let file = item.get("file").and_then(|v| v.as_str()).unwrap_or("");
                let hash = item.get("hash").and_then(|v| v.as_str()).unwrap_or("");
                if hash.is_empty() {
                    errors.push(format!("Content entry \"{file}\" is missing a hash"));
                    continue;
                }
                match by_hash.get(hash) {
                    Some(blob) => {
                        total_content_size =
                            total_content_size.saturating_add(blob.len() as i64);
                    }
                    None => errors.push(format!(
                        "The file {file} ({hash}) was not uploaded or its hash does not match its content"
                    )),
                }
            }
        }
        Some(Value::Null) | None => {}
        Some(_) => errors.push("The entity content must be an array".to_string()),
    }

    if total_content_size > MAX_WORLD_SIZE_BYTES {
        errors.push(format!(
            "The deployment exceeds the maximum world size of {} bytes",
            MAX_WORLD_SIZE_BYTES
        ));
    }

    let signer: Option<String> =
        match serde_json::from_value::<catalyrst_crypto::AuthChain>(auth_chain_value.clone()) {
            Ok(chain) => {
                match catalyrst_crypto::verify::verify_auth_chain(&chain, &entity_id, Some(now_ms))
                {
                    Ok(()) => match chain.first() {
                        Some(link) => Some(link.payload.to_lowercase()),
                        None => {
                            errors.push("The auth chain is empty".to_string());
                            None
                        }
                    },
                    Err(e) => {
                        errors.push(format!("The auth chain is invalid: {e}"));
                        None
                    }
                }
            }
            Err(e) => {
                errors.push(format!("The auth chain is malformed: {e}"));
                None
            }
        };

    if !errors.is_empty() {
        return err_response(errors);
    }

    let signer = match signer {
        Some(s) => s,
        None => return err_one("Could not recover the signer from the auth chain"),
    };
    let world_name = match normalized_world_name {
        Some(n) => n,
        None => return err_one("Missing world name"),
    };
    let label = match ownership_label {
        Some(l) => l,
        None => return err_one("Missing world name"),
    };

    let squid = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => {
            tracing::warn!(
                world = %world_name,
                signer = %signer,
                "deploy denied: squid pool unavailable, cannot resolve NAME ownership (fail-closed)"
            );
            return forbidden(
                "Not authorized: NAME-ownership verification is unavailable (deploy denied)",
            );
        }
    };

    let owner_id: Option<String> = match resolve_name_owner_id(squid, &label).await {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, label = %label, "deploy denied: squid ENS lookup failed (fail-closed)");
            return forbidden("Not authorized: could not verify NAME ownership (deploy denied)");
        }
    };

    let owns_name = owner_id
        .as_deref()
        .map(|oid| address_matches_account_id(&signer, oid))
        .unwrap_or(false);

    let acl_ok = if owns_name {
        false
    } else {
        match state.worlds.get_permission_records(&world_name).await {
            Ok(records) => records
                .iter()
                .any(|(addr, ptype)| ptype == "deployment" && addr.to_lowercase() == signer),
            Err(e) => {
                tracing::warn!(error = ?e, world = %world_name, "deploy denied: permission lookup failed (fail-closed)");
                return forbidden(
                    "Not authorized: could not verify deployment permissions (deploy denied)",
                );
            }
        }
    };

    if !owns_name && !acl_ok {
        tracing::info!(
            world = %world_name,
            signer = %signer,
            "deploy denied: signer neither owns the NAME nor holds a deployment permission"
        );
        return forbidden(format!(
            "The signer {signer} is not authorized to deploy to the world {world_name}"
        ));
    }

    let resolved_owner = owner_id
        .as_deref()
        .and_then(|oid| oid.split('-').next())
        .map(|a| a.to_lowercase())
        .unwrap_or_else(|| signer.clone());

    let mut blobs_to_store: Vec<(String, Bytes)> = Vec::new();
    blobs_to_store.push((entity_id.clone(), entity_bytes.clone()));
    if let Some(Value::Array(items)) = entity.get("content") {
        for item in items {
            if let Some(hash) = item.get("hash").and_then(|v| v.as_str()) {
                if let Some(blob) = by_hash.get(hash) {
                    blobs_to_store.push((hash.to_string(), blob.clone()));
                }
            }
        }
    }

    let contents_dir = &state.cfg.contents_dir;
    if let Err(e) = tokio::fs::create_dir_all(contents_dir).await {
        tracing::error!(error = %e, dir = %contents_dir.display(), "deploy failed: could not create contents dir");
        return internal("Failed to persist deployment content");
    }
    for (hash, bytes) in &blobs_to_store {
        if let Err(e) = store_blob(contents_dir, hash, bytes).await {
            tracing::error!(error = %e, hash = %hash, "deploy failed: could not store blob");
            return internal("Failed to persist deployment content");
        }
    }

    let auth_json = match serde_json::to_vec(&auth_chain_value) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "deploy failed: could not serialize auth chain");
            return internal("Failed to persist deployment auth chain");
        }
    };
    if let Err(e) = store_auth_file(contents_dir, &entity_id, &auth_json).await {
        tracing::error!(error = %e, "deploy failed: could not store auth file");
        return internal("Failed to persist deployment auth chain");
    }

    let parcels = pointers.clone();

    if let Err(e) = state
        .worlds
        .deploy_scene(
            &world_name,
            &resolved_owner,
            &entity_id,
            &signer,
            &auth_chain_value,
            &entity,
            &parcels,
            total_content_size,
        )
        .await
    {
        tracing::error!(error = ?e, world = %world_name, entity_id = %entity_id, "deploy failed: DB tx error");
        return internal("Failed to persist deployment");
    }

    tracing::info!(
        entity_id = %entity_id,
        signer = %signer,
        world = %world_name,
        owner = %resolved_owner,
        authz = if owns_name { "name-ownership" } else { "acl" },
        file_count = files.len(),
        content_size = total_content_size,
        user_agent = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown"),
        "POST /entities - deployed (validated + authorized + persisted)"
    );

    (
        StatusCode::OK,
        Json(json!({
            "creationTimestamp": now_ms,
            "message": format!("Deployment {entity_id} was successful, world {world_name} is now available.")
        })),
    )
}

fn address_matches_account_id(address: &str, account_id: &str) -> bool {
    account_id
        .to_lowercase()
        .starts_with(&address.to_lowercase())
}

async fn resolve_name_owner_id(
    pool: &sqlx::PgPool,
    label: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT owner_id FROM squid_marketplace.ens WHERE lower(subdomain)=lower($1)",
    )
    .bind(label)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_matching_is_case_insensitive() {
        assert!(address_matches_account_id(
            "0x959E104E1A4DB6317FA58F8295F586E1A978C297",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
        assert!(!address_matches_account_id(
            "0xdeadbeef",
            "0x959e104e1a4db6317fa58f8295f586e1a978c297-ETHEREUM"
        ));
    }

    #[test]
    fn canon_pointer_set_normalizes_and_sorts() {
        let a = canon_pointer_set(&[json!("1,2"), json!("0,0"), json!(" 0,0 ")]);
        assert_eq!(a, vec!["0,0".to_string(), "1,2".to_string()]);
    }

    #[test]
    fn canon_pointer_numerically_normalizes() {
        assert_eq!(canon_pointer("00,00"), "0,0");
        assert_eq!(canon_pointer("-0,-0"), "0,0");
        assert_eq!(canon_pointer(" 01 , 002 "), "1,2");
        assert_eq!(canon_pointer("-05,10"), "-5,10");
        assert_eq!(canon_pointer("00,00"), canon_pointer("0,0"));
        assert_eq!(canon_pointer("not-a-parcel"), "not-a-parcel");
        assert_eq!(canon_pointer("1,2,3"), "1,2,3");
        assert_eq!(canon_pointer("1e2,3"), "1e2,3");
    }

    #[test]
    fn canon_pointer_set_treats_leading_zeros_as_equal() {
        let pointers = canon_pointer_set(&[json!("00,00"), json!("01,00")]);
        let parcels = canon_pointer_set(&[json!("0,0"), json!("1,0")]);
        assert_eq!(pointers, parcels);
    }

    #[test]
    fn parcel_bounds_validation_matches_upstream() {
        assert!(validate_parcel_in_bounds("0,0").is_ok());
        assert!(validate_parcel_in_bounds("-150,150").is_ok());
        assert!(validate_parcel_in_bounds("150,-150").is_ok());
        assert!(validate_parcel_in_bounds("151,0")
            .unwrap_err()
            .contains("Coordinate X value 151 is out of bounds"));
        assert!(validate_parcel_in_bounds("0,-151")
            .unwrap_err()
            .contains("Coordinate Y value -151 is out of bounds"));
        assert!(validate_parcel_in_bounds("garbage")
            .unwrap_err()
            .contains("Invalid coordinate format"));
    }

    #[test]
    fn name_label_ownership_match_is_case_insensitive() {
        let owner_id = "0x959E104E1A4DB6317FA58f8295F586e1A978C297-ETHEREUM";
        assert!(address_matches_account_id(
            "0x959e104e1a4db6317fa58f8295f586e1a978c297",
            owner_id
        ));
        assert!(address_matches_account_id(
            "0X959E104E1A4DB6317FA58F8295F586E1A978C297",
            owner_id
        ));
        assert!(!address_matches_account_id(
            "0x0000000000000000000000000000000000000001",
            owner_id
        ));
    }

    #[test]
    fn pointers_equal_scene_parcels_after_canonicalization() {
        let pointers = canon_pointer_set(&[json!("0,0"), json!(" 1,1 "), json!("0,0")]);
        let parcels = canon_pointer_set(&[json!("1,1"), json!("0,0")]);
        assert_eq!(pointers, parcels);

        let mismatch = canon_pointer_set(&[json!("0,0"), json!("2,2")]);
        assert_ne!(pointers, mismatch);
    }

    #[test]
    fn extract_auth_chain_indexed_and_json() {
        let mut f = BTreeMap::new();
        f.insert("authChain[0][type]".into(), "SIGNER".into());
        f.insert("authChain[0][payload]".into(), "0xabc".into());
        f.insert("authChain[0][signature]".into(), "".into());
        let v = extract_auth_chain_from_fields(&f).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["type"], "SIGNER");

        let mut g = BTreeMap::new();
        g.insert(
            "authChain".into(),
            r#"[{"type":"SIGNER","payload":"0x1"}]"#.into(),
        );
        let v = extract_auth_chain_from_fields(&g).unwrap();
        assert_eq!(v[0]["payload"], "0x1");
    }

    #[test]
    fn extract_auth_chain_requires_something() {
        let f = BTreeMap::new();
        assert!(extract_auth_chain_from_fields(&f).is_err());
    }

    #[test]
    fn upload_precheck_rejects_oversized_declared_length() {
        assert!(declared_length_exceeds_limit(
            MAX_UPLOAD_SIZE_BYTES as u64 + 1
        ));
        assert!(declared_length_exceeds_limit(
            MAX_UPLOAD_SIZE_BYTES as u64 * 2
        ));
        assert!(!declared_length_exceeds_limit(MAX_UPLOAD_SIZE_BYTES as u64));
        assert!(!declared_length_exceeds_limit(
            MAX_UPLOAD_SIZE_BYTES as u64 - 1
        ));
        assert!(!declared_length_exceeds_limit(0));
        assert!(!declared_length_exceeds_limit(1024));
    }
}
