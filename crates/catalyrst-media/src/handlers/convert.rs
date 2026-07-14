use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use catalyrst_commons::http::{read_body_capped, resolve_and_pin};
use serde::Deserialize;

use crate::AppState;

const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Deserialize)]
pub struct ConvertParams {
    pub url: String,
}

fn build_response(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (status, headers, body).into_response()
}

pub async fn convert(State(state): State<AppState>, Query(p): Query<ConvertParams>) -> Response {
    if let Some((status, content_type, body)) = state.convert_cache_get(&p.url) {
        let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
        return build_response(status, &content_type, body);
    }

    let mut current = match reqwest::Url::parse(&p.url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => u,
        _ => return (StatusCode::BAD_REQUEST, "url must be http(s)").into_response(),
    };

    let mut hops = 0;
    let upstream = loop {
        let Some(pinned) = resolve_and_pin(&current).await else {
            return (StatusCode::FORBIDDEN, "url host is not publicly routable").into_response();
        };
        let host = current.host_str().unwrap_or_default().to_string();
        let client = match state.pinned_client(&host, pinned) {
            Ok(c) => c,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("client build failed: {e}"))
                    .into_response()
            }
        };
        let resp = match client.get(current.clone()).send().await {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("upstream fetch failed: {e}"),
                )
                    .into_response()
            }
        };
        if resp.status().is_redirection() {
            hops += 1;
            if hops > MAX_REDIRECTS {
                return (StatusCode::BAD_GATEWAY, "too many redirects").into_response();
            }
            let loc = resp
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok());
            let Some(loc) = loc else {
                return (StatusCode::BAD_GATEWAY, "redirect without location").into_response();
            };
            current = match current.join(loc) {
                Ok(u) if matches!(u.scheme(), "http" | "https") => u,
                _ => return (StatusCode::BAD_REQUEST, "invalid redirect target").into_response(),
            };
            continue;
        }
        break resp;
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body = match read_body_capped(upstream, MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(e) => {
            return if e.downcast_ref::<reqwest::Error>().is_some() {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("upstream read failed: {e}"),
                )
                    .into_response()
            } else {
                (StatusCode::PAYLOAD_TOO_LARGE, "source too large").into_response()
            }
        }
    };

    if status.is_success() {
        state.convert_cache_put(&p.url, status.as_u16(), &content_type, &body);
    }
    build_response(status, &content_type, body)
}
