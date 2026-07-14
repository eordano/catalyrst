use std::collections::{HashMap, HashSet};

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use serde_json::Value;

use crate::ports::worlds::SceneReplacement;
use crate::AppState;

use super::authz::{address_matches_account_id, resolve_name_owner_id};
use super::form::DeployForm;
use super::validate::{
    canon_pointer, canon_pointer_set, entity_file_too_large_error, extract_auth_chain_from_fields,
    is_canonical_parcel_set, validate_navmap_thumbnail, validate_parcel_in_bounds,
    MAX_ENTITY_FILE_SIZE_BYTES,
};
use super::{err_one, err_response, forbidden, internal, DeploySuccess};

const MAX_WORLD_SIZE_BYTES: i64 = 300 * 1024 * 1024;

const ENTITY_TTL_MS: i64 = 300_000;

/// Reject a deployment dated more than this far in the future: a garbage or clock-skewed
/// timestamp must not let an entity stay deployable/replayable indefinitely. Kept aligned
/// with the Catalyst request TTL forwards guard.
const MAX_DEPLOYMENT_FUTURE_SKEW_MS: i64 = 15 * 60 * 1000;

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
) -> Response {
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
            // One ttl, branched both ways, so a garbage timestamp can't dodge both guards.
            let ttl = now_ms.saturating_sub(ts);
            if ttl > ENTITY_TTL_MS {
                errors.push(format!(
                    "The request is not authorized to deploy: the entity timestamp is too old \
                     (older than {}s)",
                    ENTITY_TTL_MS / 1000
                ));
            } else if ttl < -MAX_DEPLOYMENT_FUTURE_SKEW_MS {
                errors.push(format!(
                    "The request is not authorized to deploy: the entity timestamp is too far \
                     in the future (more than {}s)",
                    MAX_DEPLOYMENT_FUTURE_SKEW_MS / 1000
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

    let scene_meta = entity.get("metadata").and_then(|m| m.get("scene"));
    let raw_pointers = entity.get("pointers").and_then(|v| v.as_array());
    let raw_parcels = scene_meta
        .and_then(|s| s.get("parcels"))
        .and_then(|v| v.as_array());
    let raw_base = scene_meta
        .and_then(|s| s.get("base"))
        .and_then(|b| b.as_str());

    let pointers = raw_pointers
        .map(|a| canon_pointer_set(a))
        .unwrap_or_default();
    let scene_parcels = raw_parcels
        .map(|a| canon_pointer_set(a))
        .unwrap_or_default();

    // Upstream `SceneParcels` rejects non-canonical or duplicated coordinates instead of
    // normalizing them, and pins the declared base into the scene's own parcel set -- so a
    // deployment can't be authorized/sized against a normalized set it isn't literally
    // placed on, nor claim a base outside its footprint (which would then key the wrong
    // scene identity for comms/ban lookups).
    let pointers_canonical = raw_pointers
        .map(|a| is_canonical_parcel_set(a))
        .unwrap_or(false);
    let parcels_canonical = raw_parcels
        .map(|a| is_canonical_parcel_set(a))
        .unwrap_or(false);

    if !pointers_canonical {
        errors.push(
            "The entity pointers must be a unique set of canonical parcel coordinates".to_string(),
        );
    }
    if !parcels_canonical {
        errors.push(
            "The scene parcels must be a unique set of canonical parcel coordinates".to_string(),
        );
    } else {
        let base_included = raw_base
            .map(|base| raw_parcels.is_some_and(|a| a.iter().any(|p| p.as_str() == Some(base))))
            .unwrap_or(false);
        if !base_included {
            // Verbatim upstream text, parcel list and trailing period included
            // (worlds-content-server logic/validations/scene.ts): this string is
            // the client-visible contract, and a deployer matching on it must
            // not have to special-case our node.
            let listed = raw_parcels
                .map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            errors.push(format!(
                "The scene base parcel [{}] must be included in the scene parcels [{}].",
                raw_base.unwrap_or(""),
                listed
            ));
        }
    }

    if pointers_canonical && parcels_canonical && pointers != scene_parcels {
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

    // A world-name owner may replace every overlapping scene; a parcel-scoped deployer must
    // hold permission for the full footprint of every already-deployed scene the deploy would
    // replace -- not just the new scene's own parcels -- so it can't silently remove a scene
    // reaching into parcels it was never granted. The exact set of replaced identities is
    // captured here and re-enforced under the worlds row lock inside `deploy_scene`.
    let replacement = if owns_name {
        SceneReplacement::UnrestrictedOwner
    } else {
        let overlapping = match state
            .worlds
            .scenes_overlapping_parcels(&world_name, &pointers)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = ?e, world = %world_name, "deploy denied: overlapping-scene lookup failed (fail-closed)");
                return forbidden(
                    "Not authorized: could not verify deployment permissions (deploy denied)",
                );
            }
        };
        let mut required: HashSet<String> = pointers.iter().cloned().collect();
        for scene in &overlapping {
            for parcel in &scene.parcels {
                required.insert(canon_pointer(parcel));
            }
        }

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
        // A deployment grant authorizes this deploy only if it covers EVERY required parcel:
        // world-wide grants cover all parcels, otherwise every parcel in the union of the new
        // pointers and the replaced scenes' footprints must fall inside the grantee's scoped
        // parcel set. Fail-closed.
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
            if required.iter().all(|p| granted.contains(p)) {
                authorized = true;
                break;
            }
        }

        if !authorized {
            tracing::info!(
                world = %world_name,
                signer = %signer,
                "deploy denied: signer neither owns the NAME nor holds deployment permission for the full replaced footprint"
            );
            return forbidden(format!(
                "The signer {signer} is not authorized to deploy to the world {world_name}"
            ));
        }

        SceneReplacement::Scoped(overlapping.iter().map(|s| s.entity_id.clone()).collect())
    };

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
            contents_dir,
            &replacement,
        )
        .await
    {
        // A scoped deployer whose overlap snapshot went stale gets 409 (retryable), not 500.
        if let crate::http::ApiError::Conflict(_) = e {
            tracing::info!(world = %world_name, entity_id = %entity_id, "deploy conflict: overlapping-scene set changed under a scoped authorization");
            return e.into_response();
        }
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
        Json(DeploySuccess {
            creation_timestamp: now_ms,
            message: format!(
                "Deployment {entity_id} was successful, world {world_name} is now available."
            ),
        }),
    )
        .into_response()
}
