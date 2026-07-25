use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::Utc;

use crate::dto::{
    CreditsData, CreditsProgramProgressResponse, GoalData, GoalProgressData, UserData,
};
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::ports::progress::GoalEvent;
use crate::AppState;

async fn track_login(state: &AppState, signer: &str, now: chrono::DateTime<Utc>) {
    if let Err(e) = state
        .credits
        .record_event(signer, &GoalEvent::Login, now)
        .await
    {
        tracing::warn!(error = %e, "goal progress: login event failed");
    }
}

pub async fn enroll(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let signer = signer_from(&headers, "post", "/users")?;
    state.credits.mark_started(&signer).await?;
    track_login(&state, &signer, Utc::now()).await;
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

    let now = Utc::now();
    track_login(&state, &signer, now).await;
    let has_started = state.credits.has_started(&wallet).await?;
    let credits_row = state.credits.user_credits(&wallet).await?;
    let goal_rows = state.credits.current_week_goals(&wallet, now).await?;

    let credits = match credits_row {
        Some(c) => {
            let live = c.earned_expires_at.map(|e| e > now).unwrap_or(true);
            let earned = if live { c.earned_available } else { 0.0 };
            let available = c.available - (c.earned_available - earned);
            CreditsData {
                available,
                earned,
                paid: c.available - c.earned_available,
                expires_in: match (earned > 0.0, c.earned_expires_at) {
                    (true, Some(e)) => (e - now).num_seconds().max(0) as u64,
                    _ => 0,
                },
                is_blocked_for_claiming: c.is_blocked_for_claiming,
            }
        }
        None => CreditsData {
            available: 0.0,
            earned: 0.0,
            paid: 0.0,
            expires_in: 0,
            is_blocked_for_claiming: false,
        },
    };

    let goals = goal_rows
        .into_iter()
        .map(|g| GoalData {
            title: g.title,
            description: g.description,
            thumbnail: g.thumbnail,
            progress: GoalProgressData {
                total_steps: g.total_steps.max(0) as u64,
                completed_steps: g.completed_steps.max(0) as u64,
            },
            reward: g.reward,
            is_claimed: g.is_claimed,
        })
        .collect();

    Ok(Json(CreditsProgramProgressResponse {
        user: UserData {
            has_started_program: has_started,
        },
        credits,
        goals,
    }))
}
