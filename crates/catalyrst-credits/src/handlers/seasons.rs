use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

use crate::dto::{CurrentSeasonInfo, SeasonData, SeasonsData, Week};
use crate::http::ApiError;
use crate::AppState;

pub async fn seasons(
    State(_state): State<AppState>,
    _headers: HeaderMap,
) -> Result<Json<SeasonsData>, ApiError> {
    Ok(Json(SeasonsData {
        last_season: SeasonData::default(),
        current_season: CurrentSeasonInfo {
            season: SeasonData::default(),
            week: Week::default(),
        },
        next_season: SeasonData::default(),
    }))
}
