use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use crate::ports::{normalize_details, validate_subscription_details, Subscription};
use crate::AppState;

const SCOPE_COMMUNITY: &str = "community";

pub async fn get_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Subscription>, ApiError> {
    let signer = require_signer(&headers, "get", "/subscription")?;

    let sub = match state.notifications.get_subscription(&signer).await? {
        Some(sub) => sub,
        None => Subscription {
            address: signer,
            email: None,
            unconfirmed_email: None,
            details: normalize_details(&serde_json::json!({})),
        },
    };
    Ok(Json(sub))
}

pub async fn put_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(details): Json<JsonValue>,
) -> Result<Json<Subscription>, ApiError> {
    let signer = require_signer(&headers, "put", "/subscription")?;

    validate_subscription_details(&details).map_err(ApiError::bad_request)?;

    let sub = state
        .notifications
        .put_subscription_details(&signer, &details)
        .await?;
    Ok(Json(sub))
}

#[derive(Debug, Deserialize)]
pub struct SetEmailBody {
    pub email: String,
    #[serde(rename = "isCreditsWorkflow", default)]
    pub is_credits_workflow: bool,
}

pub async fn put_set_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetEmailBody>,
) -> Result<Json<Subscription>, ApiError> {
    let signer = require_signer(&headers, "put", "/set-email")?;

    let email = body.email.trim();
    if email.is_empty() || !email.contains('@') {
        return Err(ApiError::bad_request("invalid email"));
    }

    let sub = state
        .notifications
        .set_email(&signer, email, body.is_credits_workflow)
        .await?;
    Ok(Json(sub))
}

#[derive(Debug, Deserialize)]
pub struct ConfirmEmailBody {
    #[serde(default)]
    pub address: Option<String>,
    pub code: String,
    #[serde(rename = "turnstileToken", default)]
    pub _turnstile_token: Option<String>,
    #[serde(default)]
    pub _source: Option<String>,
}

pub async fn put_confirm_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ConfirmEmailBody>,
) -> Result<Json<JsonValue>, ApiError> {
    let signer = require_signer(&headers, "put", "/confirm-email")?;

    if let Some(addr) = body.address.as_deref() {
        if !addr.eq_ignore_ascii_case(&signer) {
            return Err(ApiError::bad_request("address does not match signer"));
        }
    }

    let code = body.code.trim();
    if code.is_empty() {
        return Err(ApiError::bad_request("missing confirmation code"));
    }

    match state.notifications.confirm_email(&signer, code).await? {
        Some(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        None => Err(ApiError::bad_request("invalid or expired confirmation code")),
    }
}

pub async fn get_community_opt_out(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(community_id): Path<String>,
) -> Result<Json<JsonValue>, ApiError> {
    let path = format!("/subscription/opt-outs/community/{}", community_id);
    let signer = require_signer(&headers, "get", &path)?;

    let opted_out = state
        .notifications
        .is_opted_out(&signer, SCOPE_COMMUNITY, &community_id)
        .await?;
    Ok(Json(serde_json::json!({
        "scope": SCOPE_COMMUNITY,
        "scopeId": community_id,
        "optedOut": opted_out,
    })))
}

pub async fn delete_community_opt_out(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(community_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let path = format!("/subscription/opt-outs/community/{}", community_id);
    let signer = require_signer(&headers, "delete", &path)?;

    state
        .notifications
        .delete_opt_out(&signer, SCOPE_COMMUNITY, &community_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct OptOutBody {
    pub scope: String,
    #[serde(rename = "scopeId")]
    pub scope_id: String,
}

pub async fn post_opt_out(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OptOutBody>,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "post", "/subscription/opt-outs")?;

    if body.scope != SCOPE_COMMUNITY {
        return Err(ApiError::bad_request("unsupported opt-out scope"));
    }
    if body.scope_id.trim().is_empty() {
        return Err(ApiError::bad_request("missing scopeId"));
    }

    state
        .notifications
        .create_opt_out(&signer, &body.scope, &body.scope_id)
        .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "ok": true }))))
}
