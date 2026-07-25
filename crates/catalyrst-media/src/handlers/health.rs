use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use crate::AppStateInner;

pub async fn health(
    State(state): State<Arc<AppStateInner>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "status": if db_ok { "ok" } else { "degraded" },
            "backend": state.backend_label,
            "database": if db_ok { "ok" } else { "unreachable" },
        })),
    )
}
