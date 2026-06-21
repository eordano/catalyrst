use axum::Json;
use serde_json::Value;

use crate::http::response::ApiError;

pub async fn list_profile_settings() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /api/profiles/settings is admin-only; depends on a moderators table not yet provisioned (see docs/federation/events.md §3)",
    ))
}

pub async fn get_auth_profile_settings() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /api/profiles/me/settings requires auth-chain; will land alongside federation writes",
    ))
}

pub async fn update_my_profile_settings() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "PATCH /api/profiles/me/settings is a federation-signed action; will land with the federation phase",
    ))
}

pub async fn get_profile_settings() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /api/profiles/{id}/settings is admin-only; depends on a moderators table not yet provisioned",
    ))
}

pub async fn update_profile_settings() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "PATCH /api/profiles/{id}/settings is a federation-signed action; will land with the federation phase",
    ))
}
