use axum::extract::{Path, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};

use crate::resolver;
use crate::serve;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Response {
    let root_present = state.out_root.is_dir();
    let status = if root_present {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = Json(serde_json::json!({
        "status": if root_present { "ready" } else { "degraded" },
        "out_root": state.out_root.to_string_lossy(),
        "out_root_present": root_present,
    }));
    (status, body).into_response()
}

pub async fn dispatch(
    State(state): State<AppState>,
    method: Method,
    headers: axum::http::HeaderMap,
    Path(path): Path<String>,
) -> Response {
    let path = path.trim_start_matches('/');
    let segments: Vec<&str> = path.split('/').collect();

    if segments.first() == Some(&"manifest") && segments.len() == 2 {
        let name = segments[1];
        let Some(stem) = name.strip_suffix(".json") else {
            return serve_404();
        };
        let Some(exact) = resolver::manifest_path(&state.out_root, stem) else {
            return serve_404();
        };
        return serve::serve_manifest(&state, path, &exact, &method).await;
    }

    if segments.first() == Some(&"LOD") && segments.len() == 3 {
        let level = segments[1];
        let filename = segments[2];
        let Some(exact) = resolver::lod_path(&state.out_root, level, filename) else {
            return serve_404();
        };
        let etag = filename.strip_suffix(".br").unwrap_or(filename);
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(&state, path, &exact, etag, is_br, &method, &headers).await;
    }

    if segments.len() == 3 {
        let scene_id = segments[1];
        let filename = segments[2];
        let Some(exact) = resolver::binary_path(&state.out_root, scene_id, filename) else {
            return serve_404();
        };
        let etag = filename.strip_suffix(".br").unwrap_or(filename);
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(&state, path, &exact, etag, is_br, &method, &headers).await;
    }

    if segments.len() == 2 && segments[0] != "manifest" {
        let filename = segments[1];
        let raw = filename.strip_suffix(".br").unwrap_or(filename);
        let bare = strip_platform(raw);
        let Some(exact) = resolver::binary_path(&state.out_root, bare, filename) else {
            return serve_404();
        };
        let etag = raw;
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(&state, path, &exact, etag, is_br, &method, &headers).await;
    }

    serve_404()
}

fn strip_platform(name: &str) -> &str {
    for suffix in ["_windows", "_mac", "_linux"] {
        if let Some(s) = name.strip_suffix(suffix) {
            return s;
        }
    }
    name
}

fn serve_404() -> Response {
    let mut resp = (StatusCode::NOT_FOUND, "not found").into_response();
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    resp
}
