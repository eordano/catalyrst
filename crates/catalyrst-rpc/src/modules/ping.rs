use crate::state::AppState;
use axum::routing::get;
use axum::Router;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(|| async { "ok" }))
}
