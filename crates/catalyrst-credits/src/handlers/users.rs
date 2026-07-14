use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use crate::dto::{CreditsData, CreditsProgramProgressResponse, UserData};
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::AppState;

pub async fn enroll(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let signer = signer_from(&headers, "post", "/users")?;
    state.credits.mark_started(&signer).await?;
    Ok(StatusCode::OK)
}

pub async fn progress(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<CreditsProgramProgressResponse>, ApiError> {
    let wallet = wallet_id.to_lowercase();
    let path = format!("/users/{}/progress", wallet_id);
    let signer = signer_from(&headers, "get", &path)?;

    if signer != wallet {
        return Err(ApiError::forbidden("walletId does not match signer"));
    }

    let has_started = state.credits.has_started(&wallet).await?;
    let credits_row = state.credits.user_credits(&wallet).await?;

    // Earned credits no longer expire (the seasons domain was removed), so the
    // earned slice is always live and expiresIn is always 0. Goals were
    // season-scoped and are gone; the list stays in the wire shape, empty.
    let credits = match credits_row {
        Some(c) => CreditsData {
            available: c.available,
            earned: c.earned_available,
            paid: c.available - c.earned_available,
            expires_in: 0,
            is_blocked_for_claiming: c.is_blocked_for_claiming,
        },
        None => CreditsData {
            available: 0.0,
            earned: 0.0,
            paid: 0.0,
            expires_in: 0,
            is_blocked_for_claiming: false,
        },
    };

    Ok(Json(CreditsProgramProgressResponse {
        user: UserData {
            has_started_program: has_started,
        },
        credits,
        goals: Vec::new(),
    }))
}
