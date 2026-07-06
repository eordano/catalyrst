use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};

use crate::AppState;

pub async fn status() -> Json<Value> {
    Json(json!({
        "ok": true,
        "data": {
            "image": concat!("catalyrst-places/", env!("CARGO_PKG_VERSION")),
            "timestamp": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "version": option_env!("GIT_REV").unwrap_or(env!("CARGO_PKG_VERSION")),
        }
    }))
}

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db = state.places.ping().await.is_ok();
    let code = if db {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "ok": db,
            "version": option_env!("GIT_REV").unwrap_or(env!("CARGO_PKG_VERSION")),
            "components": { "places_db": if db { "ok" } else { "down" } },
        })),
    )
}
