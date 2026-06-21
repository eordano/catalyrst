use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use crate::ports::email::{self, CODE_LEN};
use crate::ports::{
    normalize_details, validate_subscription_details, SetEmailOutcome, Subscription,
};
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

/// PUT /set-email — signed-fetch authed. Persists the pending email and sends
/// the confirmation email. Returns 204 (empty), matching upstream.
pub async fn put_set_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetEmailBody>,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "put", "/set-email")?;

    let email = body.email.trim();
    // A non-empty email must be well-formed; an empty email is the explicit
    // "clear my email" path and is handled by set_email.
    if !email.is_empty() && (!email.contains('@') || email.starts_with('@') || email.ends_with('@'))
    {
        return Err(ApiError::bad_request("Invalid email"));
    }

    if !email.is_empty() && state.notifications.email.is_domain_blacklisted(email) {
        return Err(ApiError::bad_request("Email domain not allowed"));
    }

    match state
        .notifications
        .set_email(&signer, email, body.is_credits_workflow)
        .await?
    {
        SetEmailOutcome::NoEmailSent => {}
        SetEmailOutcome::SendConfirmation { source, code } => {
            state
                .notifications
                .email
                .send_confirmation(source, email, &signer, &code)
                .await?;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ConfirmEmailBody {
    #[serde(default)]
    pub address: Option<String>,
    pub code: String,
    #[serde(rename = "turnstileToken", default)]
    pub turnstile_token: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

/// PUT /confirm-email — intentionally UNAUTHENTICATED upstream (the email link
/// the user clicks cannot carry a signed-fetch). Identity is asserted by the
/// `{ address, code }` body plus the (optional) Turnstile challenge. Returns
/// 204 (empty) on success.
pub async fn put_confirm_email(
    State(state): State<AppState>,
    Json(body): Json<ConfirmEmailBody>,
) -> Result<impl IntoResponse, ApiError> {
    let address = body
        .address
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ApiError::bad_request("Missing address"))?;

    let code = body.code.trim();
    // Codes are fixed-length 32-char makeId strings.
    if code.len() != CODE_LEN {
        return Err(ApiError::bad_request("Invalid confirmation code"));
    }

    // Server-side Turnstile verification (no-op when no secret configured).
    let verified = email::verify_turnstile(
        state.notifications.email.turnstile_secret(),
        body.turnstile_token.as_deref(),
    )
    .await?;
    if !verified {
        return Err(ApiError::unauthorized("Turnstile verification failed"));
    }

    // `source` is accepted for wire-compatibility with the account confirm page
    // but is authoritative server-side from the pending row, not the request.
    match state.notifications.confirm_email(address, code).await? {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(ApiError::bad_request("Invalid confirmation code")),
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
