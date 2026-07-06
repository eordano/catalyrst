use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};

use crate::AppState;

pub async fn ping(OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    uri.path().to_string()
}

pub async fn status() -> Json<Value> {
    Json(json!({
        "ok": true,
        "data": {
            "image": concat!("catalyrst-worlds/", env!("CARGO_PKG_VERSION")),
            "timestamp": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "version": option_env!("GIT_REV").unwrap_or(env!("CARGO_PKG_VERSION")),
        }
    }))
}

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(state.worlds.pool())
        .await
        .is_ok();

    let body = json!({
        "ok": db_ok,
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "database": if db_ok { "healthy" } else { "unavailable" },
            "livekit": if state.cfg.livekit_configured { "configured" } else { "unconfigured" },
        },
    });

    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body))
}
