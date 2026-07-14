use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

use crate::dto::{CurrentSeasonInfo, SeasonData, SeasonsData, Week};
use crate::http::ApiError;
use crate::AppState;

// Compat stub: the seasons domain was removed (legacy), but the route stays
// because the Unity client calls it (UpdateProgramSeasonsAsync). The response
// is the legitimate "no current season" shape - all-default season/week
// objects - which clients already handle as the between-seasons state.
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
