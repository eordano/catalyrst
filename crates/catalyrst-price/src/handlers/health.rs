use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;

pub async fn health(State(state): State<AppState>) -> StatusCode {
    match catalyrst_db::ping_health(state.prices.pool()).await {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::warn!(%err, "health check db ping failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
