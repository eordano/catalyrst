use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    let relayer = state.config.has_relayer();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "database": db_ok,
        "relayer": relayer,
        "usd_pegged_stale_refusals": crate::ports::oracle::stale_refusal_count(),
    });
    (status, Json(body))
}
