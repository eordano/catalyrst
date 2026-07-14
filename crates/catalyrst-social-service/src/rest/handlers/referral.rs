//! Referral progress routes (upstream social-service-ea `/v1/referral-progress`).
//!
//! The invited user is always the signed-fetch signer: POST records the attribution
//! (first referrer wins), PATCH marks the signer as signed up, GET reports the
//! signer's own stats as a referrer. `rewardImages` stays empty on this backend --
//! nothing here grants tiers or uploads reward art, so the honest zero-activity
//! shape is also the steady state until a rewards pipeline exists.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_types::is_eth_address;
use serde::{Deserialize, Serialize};

use crate::rest::auth_chain::require_signer;
use crate::rest::handlers::error::{CommError, SignedFetchGateBody};
use crate::rest::AppState;

#[derive(Serialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct ReferralRewardImage {
    pub tier: i32,
    pub url: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct ReferralProgressStats {
    #[serde(rename = "invitedUsersAccepted")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub invited_users_accepted: i64,
    #[serde(rename = "invitedUsersAcceptedViewed")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub invited_users_accepted_viewed: i64,
    #[serde(rename = "rewardImages")]
    pub reward_images: Vec<ReferralRewardImage>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct CreateReferralBody {
    pub referrer: String,
}

#[utoipa::path(
    get,
    path = "/v1/referral-progress",
    tag = "referral",
    responses(
        (status = 200, body = ReferralProgressStats),
        (status = 400, body = SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_referral_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ReferralProgressStats>, CommError> {
    let signer = require_signer(&headers, "get", "/v1/referral-progress").await?;
    let referrer = signer.as_str().to_lowercase();

    // Upstream counts tier_granted rows; PATCH only ever reaches signed_up, so a
    // fresh backend truthfully reports zero accepted invites.
    let accepted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM referral_progress WHERE referrer = $1 AND status = 'tier_granted'",
    )
    .bind(&referrer)
    .fetch_one(&state.pool)
    .await?;

    let viewed: i64 = sqlx::query_scalar(
        "SELECT invites_accepted_viewed FROM referral_progress_viewed WHERE referrer = $1",
    )
    .bind(&referrer)
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or(0);

    // The read reports the previously seen count, then records the current one --
    // upstream uses the delta to badge "new invites accepted since last look".
    sqlx::query(
        "INSERT INTO referral_progress_viewed (referrer, invites_accepted_viewed) \
         VALUES ($1, $2) \
         ON CONFLICT (referrer) DO UPDATE \
         SET invites_accepted_viewed = EXCLUDED.invites_accepted_viewed, updated_at = now()",
    )
    .bind(&referrer)
    .bind(accepted)
    .execute(&state.pool)
    .await?;

    Ok(Json(ReferralProgressStats {
        invited_users_accepted: accepted,
        invited_users_accepted_viewed: viewed,
        reward_images: Vec::new(),
    }))
}

/// Resolves a create that found an existing row: a same-referrer duplicate is an
/// idempotent no-op (204) so client retries converge; a different referrer is a
/// genuine conflict -- attribution is first-wins.
fn resolve_existing(
    existing: &str,
    referrer: &str,
    invited: &str,
) -> Result<StatusCode, CommError> {
    if existing == referrer {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(CommError::bad_request(format!(
            "Referral progress already exists for the invited user: {invited}"
        )))
    }
}

#[utoipa::path(
    post,
    path = "/v1/referral-progress",
    tag = "referral",
    request_body(content = CreateReferralBody, description = "{ referrer }"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn create_referral(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateReferralBody>,
) -> Result<StatusCode, CommError> {
    let signer = require_signer(&headers, "post", "/v1/referral-progress").await?;
    let invited = signer.as_str().to_lowercase();
    let referrer = body.referrer.to_lowercase();

    if !is_eth_address(&referrer) {
        return Err(CommError::bad_request("Invalid referrer"));
    }
    if referrer == invited {
        return Err(CommError::bad_request(format!(
            "User cannot refer themselves: {invited}"
        )));
    }

    let existing: Option<String> =
        sqlx::query_scalar("SELECT referrer FROM referral_progress WHERE invited_user = $1")
            .bind(&invited)
            .fetch_optional(&state.pool)
            .await?;
    if let Some(existing) = existing {
        return resolve_existing(&existing, &referrer, &invited);
    }

    let inserted = sqlx::query(
        "INSERT INTO referral_progress (invited_user, referrer, status) \
         VALUES ($1, $2, 'pending') \
         ON CONFLICT (invited_user) DO NOTHING",
    )
    .bind(&invited)
    .bind(&referrer)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if inserted == 0 {
        // A concurrent create won the insert race; resolve against the stored row
        // instead of writing a second, contradictory attribution.
        let stored: Option<String> =
            sqlx::query_scalar("SELECT referrer FROM referral_progress WHERE invited_user = $1")
                .bind(&invited)
                .fetch_optional(&state.pool)
                .await?;
        return match stored {
            Some(stored) => resolve_existing(&stored, &referrer, &invited),
            None => Err(CommError::Internal),
        };
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch,
    path = "/v1/referral-progress",
    tag = "referral",
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn update_referral_signed_up(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, CommError> {
    let signer = require_signer(&headers, "patch", "/v1/referral-progress").await?;
    let invited = signer.as_str().to_lowercase();

    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM referral_progress WHERE invited_user = $1")
            .bind(&invited)
            .fetch_optional(&state.pool)
            .await?;

    match status.as_deref() {
        None => Err(CommError::not_found(format!(
            "Referral progress not found for user: {invited}"
        ))),
        // Only a pending referral can be marked signed up; a repeat PATCH after the
        // transition reports the same 400 upstream answers.
        Some(current) if current != "pending" => Err(CommError::bad_request(format!(
            "Invalid referral status: {current}. Expected: pending"
        ))),
        Some(_) => {
            sqlx::query(
                "UPDATE referral_progress SET status = 'signed_up', updated_at = now() \
                 WHERE invited_user = $1",
            )
            .bind(&invited)
            .execute(&state.pool)
            .await?;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}
