use std::net::IpAddr;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::AppState;

const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Deserialize)]
pub struct ConvertParams {
    pub url: String,
}

fn ip_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_blocked(IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

async fn resolve_pinned(url: &reqwest::Url) -> Option<std::net::SocketAddr> {
    let host = url.host_str()?;
    let port = url.port_or_known_default().unwrap_or(80);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return (!ip_blocked(ip)).then_some(std::net::SocketAddr::new(ip, port));
    }
    let addrs: Vec<_> = tokio::net::lookup_host((host, port)).await.ok()?.collect();
    if addrs.is_empty() || addrs.iter().any(|a| ip_blocked(a.ip())) {
        return None;
    }
    Some(addrs[0])
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
        let Some(pinned) = resolve_pinned(&current).await else {
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

    if let Some(len) = upstream.content_length() {
        if len as usize > MAX_BODY_BYTES {
            return (StatusCode::PAYLOAD_TOO_LARGE, "source too large").into_response();
        }
    }

    let bytes = match upstream.bytes().await {
        Ok(b) if b.len() <= MAX_BODY_BYTES => b,
        Ok(_) => return (StatusCode::PAYLOAD_TOO_LARGE, "source too large").into_response(),
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream read failed: {e}"),
            )
                .into_response()
        }
    };

    let body = bytes.to_vec();

    if status.is_success() {
        state.convert_cache_put(&p.url, status.as_u16(), &content_type, &body);
    }
    build_response(status, &content_type, body)
}
