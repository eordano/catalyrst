use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};

use crate::resolver;
use crate::serve;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Response {
    let root_present = state.out_root.is_dir();
    let live = state.live_upstream.is_some();
    // Ready if we can serve from the static tree OR fall through to the live
    // converter — in live-proxy mode an absent/empty out_root is fine.
    let ready = root_present || live;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = Json(serde_json::json!({
        "status": if ready { "ready" } else { "degraded" },
        "mode": if live { "live-proxy" } else { "static" },
        "out_root": state.out_root.to_string_lossy(),
        "out_root_present": root_present,
        "live_upstream": state.live_upstream,
    }));
    (status, body).into_response()
}

pub async fn dispatch(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    Path(path): Path<String>,
) -> Response {
    let path = path.trim_start_matches('/').to_string();
    let local = dispatch_local(&state, &path, &method, &headers).await;
    // On a local miss, fall through to the live converter (if configured) so the
    // bundle is built on demand instead of 404ing an absent static tree. This is
    // what makes ab-cdn a live-conversion proxy rather than a pure static server.
    if local.status() == StatusCode::NOT_FOUND {
        if let Some(upstream) = state.live_upstream.as_deref() {
            return proxy_upstream(&state, upstream, &path, &method, &headers).await;
        }
    }
    local
}

/// Serve a request from the local `out_root` only. Returns a 404 response on any
/// miss; the caller decides whether to fall through to the live upstream.
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

    if segments.len() == 3 {
        let scene_id = segments[1];
        let filename = segments[2];
        let Some(exact) = resolver::binary_path(&state.out_root, scene_id, filename) else {
            return serve_404();
        };
        let etag = filename.strip_suffix(".br").unwrap_or(filename);
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(state, path, &exact, etag, is_br, method, headers).await;
    }

    if segments.len() == 2 && segments[0] != "manifest" {
        let filename = segments[1];
        let raw = filename.strip_suffix(".br").unwrap_or(filename);
        let (_, bare) = resolver::split_platform(raw);
        let Some(exact) = resolver::binary_path(&state.out_root, bare, filename) else {
            return serve_404();
        };
        let etag = raw;
        let is_br = filename.ends_with(".br");
        return serve::serve_binary(state, path, &exact, etag, is_br, method, headers).await;
    }

    serve_404()
}

/// Reverse-proxy a missed request to the live converter (abgen-serve). The route
/// schemes are identical (`/manifest/<entity>_<platform>.json` and
/// `/<version>/<entity>/<file>`), so the original path is forwarded verbatim.
/// Range / conditional headers are forwarded; the relevant response headers are
/// passed through and CORS is (re)applied. The body is streamed, not buffered.
async fn proxy_upstream(
    state: &AppState,
    upstream: &str,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let url = format!("{upstream}/{path}");
    let rmethod = if *method == Method::HEAD {
        reqwest::Method::HEAD
    } else {
        reqwest::Method::GET
    };
    let mut req = state.http.request(rmethod, &url);
    for name in ["range", "if-none-match", "accept-encoding"] {
        if let Some(v) = headers.get(name).and_then(|v| v.to_str().ok()) {
            req = req.header(name, v);
        }
    }

    let upstream_resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, url = %url, "ab live upstream request failed");
            let mut resp =
                (StatusCode::BAD_GATEWAY, "ab live upstream unreachable").into_response();
            resp.headers_mut()
                .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
            return resp;
        }
    };

    let status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    // Snapshot the passthrough headers before the body consumes the response.
    const PASS: &[&str] = &[
        "content-type",
        "content-length",
        "content-encoding",
        "etag",
        "accept-ranges",
        "cache-control",
        "content-range",
    ];
    let mut saved: Vec<(&'static str, String)> = Vec::new();
    for name in PASS {
        if let Some(v) = upstream_resp
            .headers()
            .get(*name)
            .and_then(|v| v.to_str().ok())
        {
            saved.push((name, v.to_string()));
        }
    }

    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(upstream_resp.bytes_stream())
    };

    let mut resp = (status, body).into_response();
    {
        let h = resp.headers_mut();
        for (k, v) in saved {
            if let Ok(val) = v.parse() {
                h.insert(k, val);
            }
        }
        h.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        h.insert(
            "Access-Control-Expose-Headers",
            "ETag, Content-Range, Accept-Ranges, Content-Length, Content-Encoding"
                .parse()
                .unwrap(),
        );
        h.insert(
            "Access-Control-Allow-Methods",
            "GET, HEAD, OPTIONS".parse().unwrap(),
        );
    }
    resp
}

fn serve_404() -> Response {
    let mut resp = (StatusCode::NOT_FOUND, "not found").into_response();
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    resp
}
