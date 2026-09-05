use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use crate::dto::{
    CreditsData, CreditsProgramProgressResponse, CreditsTotals, UsdCredits, UserCreditsResponse,
    UserData,
};
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::AppState;

pub async fn enroll(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let signer = signer_from(&headers, "post", "/users").await?;
    state.credits.mark_started(signer.as_str()).await?;
    Ok(StatusCode::OK)
}

pub async fn progress(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<CreditsProgramProgressResponse>, ApiError> {
    let wallet = wallet_id.to_lowercase();
    let path = format!("/users/{}/progress", wallet_id);
    let signer = signer_from(&headers, "get", &path).await?;

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

pub async fn user_credits(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<UserCreditsResponse>, ApiError> {
    let wallet = wallet_id.to_lowercase();
    let path = format!("/users/{}/credits", wallet_id);
    let signer = signer_from(&headers, "get", &path).await?;

    if signer != wallet {
        return Err(ApiError::forbidden("walletId does not match signer"));
    }

    let available = state
        .credits
        .user_credits(&wallet)
        .await?
        .map(|c| c.available)
        .unwrap_or(0.0);

    Ok(Json(user_credits_response(available)))
}

fn user_credits_response(available: f64) -> UserCreditsResponse {
    UserCreditsResponse {
        credits: Vec::new(),
        total_credits: available,
        totals: CreditsTotals {
            expiring: 0.0,
            non_expiring: available,
        },
        usd: UsdCredits {
            balance_cents: (available * 100.0).round() as i64,
            credits: available.floor() as i32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_credits_wire_shape_matches_upstream_client_dto() {
        let v = serde_json::to_value(user_credits_response(12.5)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "credits": [],
                "totalCredits": 12.5,
                "totals": { "expiring": 0.0, "nonExpiring": 12.5 },
                "usd": { "balanceCents": 1250, "credits": 12 }
            })
        );
    }

    #[test]
    fn user_credits_zero_state_for_unknown_wallet() {
        let v = serde_json::to_value(user_credits_response(0.0)).unwrap();
        assert_eq!(v["totalCredits"], 0.0);
        assert_eq!(v["usd"]["credits"], 0);
        assert_eq!(v["usd"]["balanceCents"], 0);
    }
}
