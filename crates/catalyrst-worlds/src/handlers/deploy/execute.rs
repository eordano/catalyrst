use std::collections::{HashMap, HashSet};

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use bytes::Bytes;
use serde_json::{json, Value};

use crate::AppState;

use super::authz::{address_matches_account_id, resolve_name_owner_id};
use super::form::DeployForm;
use super::validate::{
    canon_pointer, canon_pointer_set, entity_file_too_large_error, extract_auth_chain_from_fields,
    validate_navmap_thumbnail, validate_parcel_in_bounds, MAX_ENTITY_FILE_SIZE_BYTES,
};
use super::{err_one, err_response, forbidden, internal};

const MAX_WORLD_SIZE_BYTES: i64 = 300 * 1024 * 1024;

const ENTITY_TTL_MS: i64 = 300_000;

/// Page size for pulling a permission's scoped parcel set; large enough to fetch every parcel in one call.
const DEPLOY_PARCEL_PAGE: i64 = 100_000;

const DCL_ETH_SUFFIX: &str = ".dcl.eth";

fn present_truthy(v: &Value, key: &str) -> bool {
    match v.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => true,
    }
}

/// Writes `bytes` to `dir/filename` via a nonce-suffixed temp file + rename, so a reader never
/// observes a partially-written file; the temp file is best-effort cleaned up on a failed rename.
async fn write_atomic(dir: &std::path::Path, filename: &str, bytes: &[u8]) -> std::io::Result<()> {
    let dst = dir.join(filename);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{filename}.{}.{nonce}.part", std::process::id()));
    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, &dst).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

async fn store_blob(dir: &std::path::Path, hash: &str, bytes: &[u8]) -> std::io::Result<()> {
    let dst = dir.join(hash);
    if tokio::fs::try_exists(&dst).await.unwrap_or(false) {
        return Ok(());
    }
    write_atomic(dir, hash, bytes).await
}

async fn store_auth_file(
    dir: &std::path::Path,
    entity_id: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    write_atomic(dir, &format!("{entity_id}.auth"), bytes).await
}

pub(super) async fn deploy_entity_inner(
    state: AppState,
    headers: HeaderMap,
    form: DeployForm,
) -> impl IntoResponse {
    let DeployForm { fields, files } = form;

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

    if entity_bytes.len() > MAX_ENTITY_FILE_SIZE_BYTES {
        return err_one(entity_file_too_large_error());
    }

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
                        total_content_size = total_content_size.saturating_add(blob.len() as i64);
                    }
                    None => {
                        // clients omit files the /available-content probe reported as stored
                        let already_stored =
                            crate::handlers::contents::is_retrievable_content_key(hash)
                                && matches!(
                                    tokio::fs::metadata(state.cfg.contents_dir.join(hash)).await,
                                    Ok(ref m) if m.is_file()
                                );
                        if already_stored {
                            let size = tokio::fs::metadata(state.cfg.contents_dir.join(hash))
                                .await
                                .map(|m| m.len() as i64)
                                .unwrap_or(0);
                            total_content_size = total_content_size.saturating_add(size);
                        } else {
                            errors.push(format!(
                                "The file {file} ({hash}) was not uploaded or its hash does not match its content"
                            ));
                        }
                    }
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

    validate_navmap_thumbnail(&entity, &mut errors);

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
        let records = match state
            .worlds
            .get_world_permission_records_full(&world_name)
            .await
        {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!(error = ?e, world = %world_name, "deploy denied: permission lookup failed (fail-closed)");
                return forbidden(
                    "Not authorized: could not verify deployment permissions (deploy denied)",
                );
            }
        };
        // A deployment grant authorizes this deploy only if it covers EVERY requested
        // pointer: world-wide grants cover all parcels, otherwise every pointer in the
        // deployment set must fall inside the grantee's scoped parcel set. Fail-closed.
        let mut authorized = false;
        for r in records.iter().filter(|r| {
            r.permission_type == "deployment" && r.address.eq_ignore_ascii_case(&signer)
        }) {
            if r.is_world_wide {
                authorized = true;
                break;
            }
            let (_total, granted) = match state
                .worlds
                .get_parcels_for_permission(r.id, DEPLOY_PARCEL_PAGE, 0, None)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = ?e, world = %world_name, "deploy denied: parcel-scope lookup failed (fail-closed)");
                    return forbidden(
                        "Not authorized: could not verify deployment permissions (deploy denied)",
                    );
                }
            };
            let granted: HashSet<String> = granted.iter().map(|p| canon_pointer(p)).collect();
            if pointers.iter().all(|p| granted.contains(p)) {
                authorized = true;
                break;
            }
        }
        authorized
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

    let resolved_name_owner: Option<String> = owner_id
        .as_deref()
        .and_then(|oid| oid.split('-').next())
        .map(|a| a.to_lowercase());

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
            resolved_name_owner.as_deref(),
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
        name_owner = ?resolved_name_owner,
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
