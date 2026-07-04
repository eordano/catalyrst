use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};

use super::resolver;
use super::serve;
use super::state::AppState;

pub async fn ping() -> &'static str {
    "ok"
}

pub async fn health(State(state): State<AppState>) -> Response {
    let root_present = state.out_root.is_dir();
    let jit = state.live_proxy.is_some();

    let jit_broken = jit && !state.live_template_ok;
    let templates_ok = state.templates_missing.is_empty();
    let ready = (templates_ok || !jit) && (jit || root_present) && !jit_broken;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = Json(serde_json::json!({
        "status": if ready { "ready" } else { "degraded" },
        "mode": if jit { "in-process" } else { "static" },
        "out_root": state.out_root.to_string_lossy(),
        "out_root_present": root_present,
        "live_inprocess": jit,
        "template_ok": state.live_template_ok,
        "templates_missing": state.templates_missing,
        "bundle_index": state.bundle_index.len(),
    }));
    (status, body).into_response()
}

pub async fn dispatch(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();
    let local = dispatch_local(&state, &path, &method, &headers).await;

    if local.status() == StatusCode::NOT_FOUND {
        if let Some(proxy) = state.live_proxy.clone() {
            if let Some((entity, platform)) = jit_target(&path) {
                let out = state.out_root.clone();
                let csu = state.manifest_content_server_url.clone();
                let (e2, p2) = (entity.clone(), platform.clone());
                let built = tokio::task::spawn_blocking(move || {
                    proxy.build_entity_into_corpus(&out, &e2, &p2, &csu)
                })
                .await;
                match built {
                    Ok(Ok(_)) => {
                        state.resolve_cache.invalidate(&path).await;
                        return dispatch_local(&state, &path, &method, &headers).await;
                    }
                    Ok(Err(e)) => tracing::warn!(
                        error = %format!("{e:#}"),
                        entity = %entity,
                        "jit write-back failed"
                    ),
                    Err(e) => tracing::error!(error = %e, "jit build worker panicked"),
                }
            }
        }
    }
    local
}

fn jit_target(path: &str) -> Option<(String, String)> {
    let segs: Vec<&str> = path.split('/').collect();

    if segs.len() == 2 && segs[0] == "manifest" {
        let stem = segs[1].strip_suffix(".json")?;
        let (entity, platform) = stem.rsplit_once('_')?;
        if !is_platform(platform) || !resolver::is_safe_component(entity) {
            return None;
        }
        return Some((entity.to_string(), platform.to_string()));
    }

    if segs.len() == 3 && segs[0] != "manifest" && segs[0] != "LOD" {
        let entity = segs[1];
        let raw = segs[2].strip_suffix(".br").unwrap_or(segs[2]);
        if !is_bundle_name(raw) || !resolver::is_safe_component(entity) {
            return None;
        }
        return Some((entity.to_string(), resolver::platform_of(raw).to_string()));
    }
    None
}

fn is_platform(p: &str) -> bool {
    matches!(p, "windows" | "mac" | "linux" | "webgl")
}

async fn dispatch_local(
    state: &AppState,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let segments: Vec<&str> = path.split('/').collect();

    if segments.first() == Some(&"manifest") && segments.len() == 2 {
        let name = segments[1];
        let Some(stem) = name.strip_suffix(".json") else {
            return serve_404();
        };
        let Some(exact) = resolver::manifest_path(&state.out_root, stem) else {
            return serve_404();
        };
        return serve::serve_manifest(state, path, &exact, method).await;
    }

    if segments.first() == Some(&"LOD") && segments.len() == 3 {
        let level = segments[1];
        let filename = segments[2];
        let Some(exact) = resolver::lod_path(&state.out_root, level, filename) else {
            return serve_404();
        };
        let etag = filename.strip_suffix(".br").unwrap_or(filename);
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(state, path, &exact, etag, is_br, method, headers).await;
    }

    if segments.len() == 3 && segments[0] != "manifest" {
        let entity = segments[1];
        let filename = segments[2];
        let raw = filename.strip_suffix(".br").unwrap_or(filename);
        if is_bundle_name(raw) {
            let is_br = filename.ends_with(".br");
            let Some(exact) = resolver::binary_path(&state.out_root, entity, filename) else {
                return serve_404();
            };
            return serve::serve_binary(state, path, &exact, raw, is_br, method, headers).await;
        }

        if let Some(resp) = serve_content_native(state, entity, filename, method).await {
            return resp;
        }
        return serve_404();
    }

    if segments.len() == 2 && segments[0] != "manifest" {
        let filename = segments[1];
        let raw = filename.strip_suffix(".br").unwrap_or(filename);
        let is_br = filename.ends_with(".br");
        let (_, bare) = resolver::split_platform(raw);
        let exact = state
            .bundle_index
            .get(&filename.to_ascii_lowercase())
            .cloned()
            .or_else(|| resolver::binary_path(&state.out_root, bare, filename));
        let Some(exact) = exact else {
            return serve_404();
        };
        return serve::serve_binary(state, path, &exact, raw, is_br, method, headers).await;
    }

    if segments.len() >= 4 && segments[0] != "manifest" && segments[0] != "LOD" {
        let entity = segments[1];
        let file = segments[2..].join("/");
        if let Some(resp) = serve_content_native(state, entity, &file, method).await {
            return resp;
        }
        return serve_404();
    }

    serve_404()
}

fn is_bundle_name(raw: &str) -> bool {
    matches!(resolver::split_platform(raw).0, "windows" | "mac" | "linux") || !raw.contains('.')
}

async fn serve_content_native(
    state: &AppState,
    entity: &str,
    file: &str,
    method: &Method,
) -> Option<Response> {
    if !resolver::is_safe_component(entity) {
        return None;
    }
    for seg in file.split('/') {
        if !resolver::is_safe_component(seg) {
            return None;
        }
    }
    let st = state.clone();
    let entity_q = entity.to_string();
    let file_q = file.to_string();
    let bytes = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
        let scene = st.content.fetch_entity(&entity_q).ok()?;
        let hash = scene
            .content
            .iter()
            .find(|c| c.file.eq_ignore_ascii_case(&file_q))
            .map(|c| c.hash.clone())?;
        st.content.fetch_content(&hash).ok()
    })
    .await
    .ok()
    .flatten()?;

    let len = bytes.len();
    let mut resp = if *method == Method::HEAD {
        StatusCode::OK.into_response()
    } else {
        (StatusCode::OK, bytes).into_response()
    };
    let h = resp.headers_mut();
    h.insert("Content-Type", content_type_for(file).parse().unwrap());
    h.insert("Content-Length", len.to_string().parse().unwrap());
    h.insert("Cache-Control", "public, max-age=600".parse().unwrap());
    h.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    Some(resp)
}

fn content_type_for(file: &str) -> &'static str {
    let f = file.to_ascii_lowercase();
    if f.ends_with(".json") {
        "application/json"
    } else if f.ends_with(".js") {
        "application/javascript"
    } else {
        "application/octet-stream"
    }
}

fn serve_404() -> Response {
    let mut resp = (StatusCode::NOT_FOUND, "not found").into_response();
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    resp
}

const MAX_POINTERS: usize = 200;

fn parse_pointers(body: &serde_json::Value) -> Result<Vec<String>, (StatusCode, &'static str)> {
    let pointers: Vec<String> = body
        .get("pointers")
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if pointers.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "pointers must be a non-empty array",
        ));
    }
    if pointers.len() > MAX_POINTERS {
        return Err((StatusCode::BAD_REQUEST, "too many pointers"));
    }
    Ok(pointers)
}

struct ResolvedEntity {
    entity_id: String,
    entity_type: String,
    timestamp: i64,
    pointers: Vec<String>,
    content: Vec<(String, String)>,
    metadata: serde_json::Value,
    deployer: String,
}

async fn resolve_entities(state: &AppState, pointers: Vec<String>) -> Vec<ResolvedEntity> {
    if let Some(cdb) = &state.content_db {
        match cdb.resolve_pointers(&pointers).await {
            Ok(ents) => ents
                .into_iter()
                .map(|e| ResolvedEntity {
                    entity_id: e.entity_id,
                    entity_type: e.entity_type,
                    timestamp: e.timestamp,
                    pointers: e.pointers,
                    content: e.content.into_iter().map(|c| (c.file, c.hash)).collect(),
                    metadata: e.metadata,
                    deployer: e
                        .deployer_address
                        .map(|d| d.to_lowercase())
                        .unwrap_or_default(),
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "folded index: content-db resolve_pointers failed");
                Vec::new()
            }
        }
    } else {
        let st = state.clone();
        tokio::task::spawn_blocking(move || {
            pointers
                .iter()
                .filter_map(|p| {
                    let s = st.content.resolve_scene(p).ok()?;
                    Some(ResolvedEntity {
                        entity_id: s.entity_id,
                        entity_type: s.entity_type,
                        timestamp: 0,
                        pointers: s.pointers,
                        content: s.content.into_iter().map(|c| (c.file, c.hash)).collect(),
                        metadata: s.metadata,
                        deployer: String::new(),
                    })
                })
                .collect()
        })
        .await
        .unwrap_or_default()
    }
}

fn entity_buildable(content: &[(String, String)]) -> bool {
    content.iter().any(|(f, _)| super::index::is_convertible(f))
}

pub async fn post_entities_versions(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let pointers = match parse_pointers(&body) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let ents = resolve_entities(&state, pointers).await;

    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || {
        ents.into_iter()
            .filter_map(|e| {
                let (versions, bundles, status) = super::index::entity_ab_record(
                    &st.out_root,
                    &st.bundle_index,
                    &e.entity_id,
                    entity_buildable(&e.content),
                    &st.ab_version,
                    &st.ab_date,
                )?;
                Some(serde_json::json!({
                    "pointers": e.pointers,
                    "versions": versions,
                    "bundles": bundles,
                    "status": status,
                }))
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    Json(out).into_response()
}

pub async fn post_entities_active(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let pointers = match parse_pointers(&body) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let ents = resolve_entities(&state, pointers).await;

    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || {
        ents.into_iter()
            .filter_map(|e| {
                let (versions, bundles, status) = super::index::entity_ab_record(
                    &st.out_root,
                    &st.bundle_index,
                    &e.entity_id,
                    entity_buildable(&e.content),
                    &st.ab_version,
                    &st.ab_date,
                )?;
                let content: Vec<serde_json::Value> = e
                    .content
                    .iter()
                    .map(|(f, h)| serde_json::json!({ "file": f, "hash": h }))
                    .collect();
                Some(serde_json::json!({
                    "id": e.entity_id,
                    "type": e.entity_type,
                    "timestamp": e.timestamp,
                    "pointers": e.pointers,
                    "content": content,
                    "metadata": e.metadata,
                    "deployer": e.deployer,
                    "status": status,
                    "bundles": bundles,
                    "versions": versions,
                }))
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    Json(out).into_response()
}

#[cfg(test)]
mod tests {
    use super::jit_target;

    #[test]
    fn jit_target_manifest_route() {
        assert_eq!(
            jit_target("manifest/bafkreiEntity_windows.json"),
            Some(("bafkreiEntity".to_string(), "windows".to_string()))
        );

        assert_eq!(jit_target("manifest/bafkreiEntity_bogus.json"), None);
        assert_eq!(jit_target("manifest/noplatform.json"), None);
    }

    #[test]
    fn jit_target_bundle_route() {
        assert_eq!(
            jit_target("v41/bafkEntity/Qmhash_mac"),
            Some(("bafkEntity".to_string(), "mac".to_string()))
        );

        assert_eq!(
            jit_target("v41/bafkEntity/Qmhash_linux.br"),
            Some(("bafkEntity".to_string(), "linux".to_string()))
        );
    }

    #[test]
    fn jit_target_skips_native_and_flat() {
        assert_eq!(jit_target("v41/bafkEntity/scene.json"), None);
        assert_eq!(jit_target("v41/bafkEntity/main.crdt"), None);

        assert_eq!(jit_target("v41/Qmhash_windows"), None);

        assert_eq!(jit_target("LOD/2/scene_2_windows"), None);
    }
}
