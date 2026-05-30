use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};

use crate::dto::{CurrentSeasonInfo, SeasonData, SeasonsData, Week};
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::ports::credits::{SeasonRow, WeekRow};
use crate::AppState;

pub async fn seasons(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SeasonsData>, ApiError> {
    let _signer = signer_from(&headers, "get", "/seasons")?;

    let now = Utc::now();
    let last = state.credits.last_season(now).await?;
    let current = state.credits.current_season(now).await?;
    let next = state.credits.next_season(now).await?;

    let (current_season, week) = match &current {
        Some(s) => {
            let w = state.credits.current_week(s.id, now).await?;
            (season_to_dto(s, now), week_to_dto(w.as_ref(), now))
        }
        None => (SeasonData::default(), Week::default()),
    };

    Ok(Json(SeasonsData {
        last_season: last.as_ref().map(|s| season_to_dto(s, now)).unwrap_or_default(),
        current_season: CurrentSeasonInfo {
            season: current_season,
            week,
        },
        next_season: next.as_ref().map(|s| season_to_dto(s, now)).unwrap_or_default(),
    }))
}

fn season_to_dto(s: &SeasonRow, now: DateTime<Utc>) -> SeasonData {
    SeasonData {
        id: s.id,
        name: s.name.clone(),
        start_date: s.start_date.to_rfc3339(),
        end_date: s.end_date.to_rfc3339(),
        max_mana: format_numeric(s.max_mana),
        time_left: (s.end_date - now).num_seconds().max(0),
        amount_of_weeks: s.amount_of_weeks,
        state: s.state.clone(),
    }
}

fn week_to_dto(w: Option<&WeekRow>, now: DateTime<Utc>) -> Week {
    match w {
        Some(w) => {
            let remaining = (w.end_date - now).num_seconds().max(0) as u64;
            Week {
                week_number: w.week_number,
                time_left: remaining,
                start_date: w.start_date.to_rfc3339(),
                end_date: w.end_date.to_rfc3339(),
                seconds_remaining: remaining,
            }
        }
        None => Week::default(),
    }
}

fn format_numeric(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}
