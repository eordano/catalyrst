use super::*;

pub async fn dispatch(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();
    let local = dispatch_local(&state, &path, &method, &headers).await;

    if local.status() == StatusCode::NOT_FOUND {
        if let Some(target) = resolver::shader_target(&path) {
            return shader_fallback(&state, &path, &target, &method, &headers, local).await;
        }
        if let Some(proxy) = state.live_proxy.clone() {
            if let Some(target) = jit_target(&path) {
                return bundle_fallback(&state, proxy, &path, &target, &method, &headers, local)
                    .await;
            }
            if br_bundle_target(&path) {
                return with_reason(local, "br-not-built");
            }
        }
        if path.split('/').next() == Some("LOD") {
            return lod_fallback(&state, &path, &method, &headers, local).await;
        }
        let segs: Vec<&str> = path.split('/').collect();
        if segs.len() == 3 && segs[0] == "lods-unity" && segs[1] == "manifests" {
            return iss_fallback(&state, &path, &method, &headers, local).await;
        }
        if let Some((bare, platform)) = flat_target(&path) {
            return flat_fallback(&state, &path, &bare, &platform, &method, &headers, local).await;
        }
    }
    local
}

pub(super) async fn dispatch_local(
    state: &AppState,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let segments: Vec<&str> = path.split('/').collect();

    if segments.first() == Some(&"manifest") && segments.len() == 2 {
        let name = segments[1];
        let Some(stem) = name.strip_suffix(".json") else {
            return serve::not_found();
        };
        let Some(exact) = resolver::manifest_path(&state.out_root, stem) else {
            return serve::not_found();
        };
        return serve::serve_manifest(state, path, &exact, method).await;
    }

    if segments.first() == Some(&"LOD") && segments.len() == 3 {
        let level = segments[1];
        let filename = segments[2];
        let Some(exact) = resolver::lod_path(&state.out_root, level, filename) else {
            return serve::not_found();
        };
        let etag = filename.strip_suffix(".br").unwrap_or(filename);
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(state, path, &exact, etag, is_br, method, headers).await;
    }

    if segments.len() == 3 && segments[0] == "lods-unity" && segments[1] == "manifests" {
        let filename = segments[2];
        let Some(exact) = resolver::iss_manifest_path(&state.out_root, filename) else {
            return serve::not_found();
        };
        return serve::serve_manifest(state, filename, &exact, method).await;
    }

    if segments.len() == 3 && segments[0] != "manifest" {
        let entity = segments[1];
        let filename = segments[2];
        let raw = filename.strip_suffix(".br").unwrap_or(filename);
        if is_bundle_name(raw) {
            let is_br = filename.ends_with(".br");
            let Some(exact) = resolver::binary_path(&state.out_root, entity, filename) else {
                return serve::not_found();
            };
            return serve::serve_binary(state, path, &exact, raw, is_br, method, headers).await;
        }

        if let Some(resp) = serve_content_native(state, entity, filename, method).await {
            return resp;
        }
        return serve::not_found();
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
            return serve::not_found();
        };
        return serve::serve_binary(state, path, &exact, raw, is_br, method, headers).await;
    }

    if segments.len() >= 4 && segments[0] != "manifest" && segments[0] != "LOD" {
        let entity = segments[1];
        let file = segments[2..].join("/");
        if let Some(resp) = serve_content_native(state, entity, &file, method).await {
            return resp;
        }
        return serve::not_found();
    }

    serve::not_found()
}

pub(super) fn is_bundle_name(raw: &str) -> bool {
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
