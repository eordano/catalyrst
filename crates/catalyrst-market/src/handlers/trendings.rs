use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::trendings::{parse_filters, TrendingSale};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct TrendingsEnvelope {
    pub data: Vec<TrendingSale>,
}

pub async fn get_trendings(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<TrendingsEnvelope>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let data = state.trendings.fetch(&filters).await?;
    Ok(Json(TrendingsEnvelope { data }))
}
