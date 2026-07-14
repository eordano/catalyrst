use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = catalyrst_db::ping_health(&state.pool).await.is_ok();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "database": db_ok,
        "relayer": state.config.can_relay(),
        "relayer_mode": state.config.relay_mode(),
        "usd_pegged_stale_refusals": crate::ports::oracle::stale_refusal_count(),
    });
    (status, Json(body))
}
