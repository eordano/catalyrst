use axum::routing::get;
use axum::Json;
use axum::Router;
use serde_json::json;

pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/ping", get(|| async { "/ping" }))
        .route(
            "/health",
            get(|| async {
                Json(json!({
                    "status": "ok",
                    "service": "catalyrst-explorer-api",
                }))
            }),
        )
}
