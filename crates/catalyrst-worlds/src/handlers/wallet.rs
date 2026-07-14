use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::AppState;

#[utoipa::path(
    get,
    path = "/wallet/{wallet}/connected-world",
    tag = "wallet",
    params(("wallet" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn connected_world(
    State(state): State<AppState>,
    Path(wallet): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state.presence.get_peer_world(&wallet) {
        Some(world) => Ok(Json(json!({ "wallet": wallet, "world": world }))),
        None => Err(ApiError::not_found(format!(
            "Wallet {} is not connected to any world",
            wallet
        ))),
    }
}
