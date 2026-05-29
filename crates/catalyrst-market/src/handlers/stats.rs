use axum::extract::{Path, Query, State};
use axum::Json;

use crate::http::response::ApiError;
use crate::ports::stats::{parse_category, parse_filters, parse_stat, StatsEnvelope};
use crate::AppState;

pub async fn get_stats(
    State(state): State<AppState>,
    Path((category, stat)): Path<(String, String)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<StatsEnvelope>, ApiError> {
    let cat = parse_category(&category);
    let st = parse_stat(&stat);
    let filters = parse_filters(&pairs)?;
    let data = state.stats.fetch(cat, st, &filters).await?;
    Ok(Json(StatsEnvelope { data }))
}
