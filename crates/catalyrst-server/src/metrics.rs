use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub fn init() {
    if HANDLE.get().is_some() {
        return;
    }
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            let _ = HANDLE.set(handle);
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to install prometheus recorder; /metrics disabled")
        }
    }
}

pub async fn metrics_handler() -> Response {
    match HANDLE.get() {
        Some(h) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            h.render(),
        )
            .into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "metrics not initialized").into_response(),
    }
}

pub async fn track_http(req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned())
        .unwrap_or_else(|| "<unmatched>".to_owned());
    let start = Instant::now();
    let resp = next.run(req).await;
    let status = resp.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();
    metrics::counter!(
        "catalyrst_http_requests_total",
        "method" => method.clone(), "route" => route.clone(), "status" => status
    )
    .increment(1);
    metrics::histogram!(
        "catalyrst_http_request_duration_seconds",
        "method" => method, "route" => route
    )
    .record(elapsed);
    resp
}
