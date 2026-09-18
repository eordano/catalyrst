use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderName};
use axum::Json;

use crate::dto::{CurrentSeasonInfo, SeasonData, SeasonsData, Week};
use crate::http::ApiError;
use crate::AppState;

pub async fn seasons(
    State(_state): State<AppState>,
    _headers: HeaderMap,
) -> Result<([(HeaderName, &'static str); 1], Json<SeasonsData>), ApiError> {
    // Static placeholder data: let clients and proxies hold it for a while.
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=300")],
        Json(SeasonsData {
            last_season: SeasonData::default(),
            current_season: CurrentSeasonInfo {
                season: SeasonData::default(),
                week: Week::default(),
            },
            next_season: SeasonData::default(),
        }),
    ))
}
