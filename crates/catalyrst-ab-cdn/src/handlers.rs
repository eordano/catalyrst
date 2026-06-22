use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};

use crate::resolver;
use crate::serve;
use crate::state::AppState;

pub async fn ping() -> &'static str {
    "ok"
}

pub async fn health(State(state): State<AppState>) -> Response {
    let root_present = state.out_root.is_dir();
    let jit = state.live_proxy.is_some();
    // In-process JIT with a missing build template is broken: every corpus miss
    // 500s. Report degraded so the misconfig is caught at the health check rather
    // than as silent per-request failures (the wearable-500 footgun).
    let jit_broken = jit && !state.live_template_ok;
    let ready = (root_present || jit) && !jit_broken;
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
        "bundle_index": state.bundle_index.len(),
    }));
    (status, body).into_response()
}

/// AB-serve handler — mounted as the router FALLBACK so the folded registry's
/// specific routes (profiles, worlds, …) take precedence and everything else
/// (manifests, LOD, `<v>/<entity>/<file>`, flat no-deps) is served here. Reads the
/// path from the URI (AB paths are content-addressed; no percent-encoding).
pub async fn dispatch(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();
    let local = dispatch_local(&state, &path, &method, &headers).await;
    // On a corpus miss, build the entity in-process and write it into the corpus,
    // then RE-SERVE via the same corpus path. The response is produced by serve.rs
    // exactly as for a batch-built hit — same bytes, content-type
    // (application/wasm), immutable cache-control, ETag, range — so corpus-hit and
    // JIT-miss are indistinguishable. Requests with no resolvable entity (native
    // content, the legacy flat no-deps `<hash>_<platform>` URL) just 404 — there is
    // no abgen-serve fallback anymore; in-process is the only converter.
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
                        // Drop the negative resolve-cache entry seeded by the first
                        // (pre-build) miss so the freshly-written file resolves.
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

/// Derive the `(entity_id, platform)` a corpus-missed request maps to, for JIT
/// write-back. Only bundle + manifest routes have a resolvable entity; native
/// content (scene.json / main.crdt / *.js) and the flat no-deps bundle URL
/// (entity not derivable from the path) return None and fall through unchanged.
fn jit_target(path: &str) -> Option<(String, String)> {
    let segs: Vec<&str> = path.split('/').collect();
    // /manifest/<entity>_<platform>.json
    if segs.len() == 2 && segs[0] == "manifest" {
        let stem = segs[1].strip_suffix(".json")?;
        let (entity, platform) = stem.rsplit_once('_')?;
        if !is_platform(platform) || !resolver::is_safe_component(entity) {
            return None;
        }
        return Some((entity.to_string(), platform.to_string()));
    }
    // /<version>/<entity>/<file> where <file> is a bundle (not native content)
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

/// Serve a request from the local `out_root` (+ disk-or-remote content) only.
/// Returns a 404 response on any miss; the caller decides whether to fall through
/// to the live upstream.
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

    // /<version>/<entity>/<file...> — bundle (with-deps) OR native content file.
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
        // Native content passthrough (scene.json / main.crdt / *.js / …): resolve
        // the entity, map file -> content hash, serve the bytes (disk-or-remote).
        if let Some(resp) = serve_content_native(state, entity, filename, method).await {
            return resp;
        }
        return serve_404();
    }

    // /<version>/<hash>_<platform>[.br] — legacy v0-abgen no-deps bundle URL.
    // Resolve via the no-deps index (deps stripped) first; this is the path the
    // hardlink aliases used to cover. Fall back to the flat/subdir layout.
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

    // /<version>/<entity>/<dir>/<file> (e.g. bin/index.js) — native content only.
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

/// A bundle filename is either platform-suffixed (`_windows`/`_mac`/`_linux`) or
/// an extension-less content-id (webgl bundle). Anything with a `.` extension
/// (scene.json, main.crdt, *.js) is a content file, not a bundle.
fn is_bundle_name(raw: &str) -> bool {
    matches!(resolver::split_platform(raw).0, "windows" | "mac" | "linux") || !raw.contains('.')
}

/// Native content passthrough: resolve `<entity>` -> entity JSON, map `<file>` ->
/// its content hash, fetch the bytes from the disk-or-remote content source, and
/// serve them. Replaces the nginx scene.json/main.crdt/bin-index passthrough.
/// Returns None on any miss so the caller can 404 (and fall to live upstream).
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
    h.insert(
        "Cache-Control",
        "public, max-age=600".parse().unwrap(),
    );
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

/// Parse + validate the `{pointers:[...]}` body (non-empty, ≤ MAX_POINTERS).
/// Returns a small `(status, message)` on error (kept small to avoid a large
/// `Result` Err variant).
fn parse_pointers(body: &serde_json::Value) -> Result<Vec<String>, (StatusCode, &'static str)> {
    let pointers: Vec<String> = body
        .get("pointers")
        .and_then(|p| p.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if pointers.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "pointers must be a non-empty array"));
    }
    if pointers.len() > MAX_POINTERS {
        return Err((StatusCode::BAD_REQUEST, "too many pointers"));
    }
    Ok(pointers)
}

/// Common resolved-entity shape for the folded index. From the content DB (full
/// fidelity: real `timestamp`/`deployer`, same query as the registry) or, when no
/// content DB is configured, the content client (fallback: `timestamp` 0,
/// `deployer` "").
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
        // Content-DB path: byte-exact with the registry (same resolve_pointers query).
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
                    deployer: e.deployer_address.map(|d| d.to_lowercase()).unwrap_or_default(),
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "folded index: content-db resolve_pointers failed");
                Vec::new()
            }
        }
    } else {
        // Fallback: content client (no timestamp/deployer).
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
    content.iter().any(|(f, _)| crate::index::is_convertible(f))
}

/// Folded AB-availability index. Resolves pointers→entities (content DB, or the
/// content client as fallback), then derives JIT-aware, servability-checked AB
/// records from THIS server's own `out_root` — so the index can never disagree
/// with what the server serves (corpus-hit or JIT-on-miss). Entities with no
/// AB-able content are skipped, as the registry skips non-servable ones.
///
/// Wire-compatible with the registry's `EntityVersions` ({pointers, versions,
/// bundles, status}).
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
                let (versions, bundles, status) = crate::index::entity_ab_record(
                    &st.out_root,
                    &st.bundle_index,
                    &e.entity_id,
                    entity_buildable(&e.content),
                    &st.ab_version,
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

/// Folded AB-registry `/entities/active`: the entity plus its JIT-aware,
/// servability-derived AB record, from THIS server's corpus. Wire-compatible with
/// the registry's `DbEntity` shape. With a content DB configured, `timestamp` and
/// `deployer` are byte-exact with the registry; without one, they fall back to
/// 0/"" (AB fields versions/bundles/status are exact either way).
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
                let (versions, bundles, status) = crate::index::entity_ab_record(
                    &st.out_root,
                    &st.bundle_index,
                    &e.entity_id,
                    entity_buildable(&e.content),
                    &st.ab_version,
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
        // bad platform / not a manifest -> no JIT build
        assert_eq!(jit_target("manifest/bafkreiEntity_bogus.json"), None);
        assert_eq!(jit_target("manifest/noplatform.json"), None);
    }

    #[test]
    fn jit_target_bundle_route() {
        // /<version>/<entity>/<hash>_<platform>
        assert_eq!(
            jit_target("v41/bafkEntity/Qmhash_mac"),
            Some(("bafkEntity".to_string(), "mac".to_string()))
        );
        // brotli suffix is stripped before platform resolution
        assert_eq!(
            jit_target("v41/bafkEntity/Qmhash_linux.br"),
            Some(("bafkEntity".to_string(), "linux".to_string()))
        );
    }

    #[test]
    fn jit_target_skips_native_and_flat() {
        // native content (has a dot extension) -> served live from content store,
        // not an AB build target
        assert_eq!(jit_target("v41/bafkEntity/scene.json"), None);
        assert_eq!(jit_target("v41/bafkEntity/main.crdt"), None);
        // flat no-deps URL: entity not derivable from the path
        assert_eq!(jit_target("v41/Qmhash_windows"), None);
        // LOD route is not a JIT target
        assert_eq!(jit_target("LOD/2/scene_2_windows"), None);
    }
}
