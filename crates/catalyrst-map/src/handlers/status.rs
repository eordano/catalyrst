use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::AppState;

pub async fn ping() -> &'static str {
    "ok"
}

pub async fn ready(State(state): State<AppState>) -> Response {
    if state.map.is_ready() {
        (StatusCode::OK, "ok").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response()
    }
}
