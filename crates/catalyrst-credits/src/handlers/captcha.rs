use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, Utc};
use sqlx::Row;

use crate::captcha::{answer_for_seed, render_png, CLAIM_TOLERANCE};
use crate::dto::{ClaimCreditsBody, ClaimCreditsResponse};
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::AppState;

const CAPTCHA_TTL_SECS: i64 = 120;

pub async fn generate(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let signer = signer_from(&headers, "get", "/captcha")?;

    let now = Utc::now();
    let expires_at = now + Duration::seconds(CAPTCHA_TTL_SECS);
    let seed = (now.timestamp_millis() as u64)
        ^ signer
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    let answer = answer_for_seed(seed);

    sqlx::query(
        "UPDATE captcha_challenges SET consumed_at = now() \
         WHERE address = $1 AND consumed_at IS NULL",
    )
    .bind(&signer)
    .execute(&state.credits.pool)
    .await?;

    sqlx::query(
        "INSERT INTO captcha_challenges (address, answer_x, expires_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(&signer)
    .bind(answer)
    .bind(expires_at)
    .execute(&state.credits.pool)
    .await?;

    let png = render_png(answer);
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], png).into_response())
}

pub async fn claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<ClaimCreditsBody>>,
) -> Result<Response, ApiError> {
    let signer = signer_from(&headers, "post", "/captcha")?;
    let Json(claim) = body.ok_or_else(|| ApiError::bad_request("missing JSON body { x }"))?;

    let now = Utc::now();
    let row = sqlx::query(
        "UPDATE captcha_challenges SET consumed_at = now() \
         WHERE id = ( \
             SELECT id FROM captcha_challenges \
             WHERE address = $1 AND consumed_at IS NULL AND expires_at > $2 \
             ORDER BY id DESC LIMIT 1 \
         ) \
         AND consumed_at IS NULL \
         RETURNING answer_x::float8 AS answer_x",
    )
    .bind(&signer)
    .bind(now)
    .fetch_optional(&state.credits.pool)
    .await?;

    let answer = row
        .map(|r| r.get::<f64, _>("answer_x"))
        .ok_or_else(|| ApiError::bad_request("no active captcha challenge"))?;

    if (answer - claim.x).abs() > CLAIM_TOLERANCE {
        return Ok(Json(ClaimCreditsResponse {
            ok: false,
            credits_granted: 0.0,
            is_blocked_for_claiming: false,
        })
        .into_response());
    }

    let outcome = state.credits.claim_credits(&signer).await?;

    Ok(Json(ClaimCreditsResponse {
        ok: outcome.ok,
        credits_granted: outcome.credits_granted,
        is_blocked_for_claiming: outcome.is_blocked_for_claiming,
    })
    .into_response())
}
