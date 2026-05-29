use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::analytics_day_data::{get_timestamp_from_timeframe, AnalyticsTimeframe};
use crate::ports::rankings::{parse_filters, RankingEntity, RankingResponse};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct RankingsEnvelope {
    pub data: RankingResponse,
}

pub async fn get_rankings(
    State(state): State<AppState>,
    Path((entity, timeframe)): Path<(String, String)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<RankingsEnvelope>, ApiError> {
    let entity = RankingEntity::from_str(&entity).ok_or_else(|| {
        ApiError::bad_request(format!(
            "Entity not supported: {}. Supported entities are: wearables, emotes, creators, collectors",
            entity
        ))
    })?;
    let from = get_timestamp_from_timeframe(AnalyticsTimeframe::from_str(&timeframe));
    let filters = parse_filters(&pairs, from)?;
    let data = state.rankings.fetch(entity, &filters).await?;
    Ok(Json(RankingsEnvelope { data }))
}
