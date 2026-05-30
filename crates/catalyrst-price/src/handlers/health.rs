use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;

pub async fn health(State(state): State<AppState>) -> StatusCode {
    match sqlx::query("SELECT 1")
        .execute(state.prices.pool())
        .await
    {
        Ok(_) => StatusCode::OK,
        Err(err) => {
            tracing::warn!(%err, "health check db ping failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
