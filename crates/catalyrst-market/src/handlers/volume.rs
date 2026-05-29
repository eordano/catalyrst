use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::analytics_day_data::AnalyticsTimeframe;
use crate::ports::volume::VolumeData;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct VolumeEnvelope {
    pub data: VolumeData,
}

pub async fn get_volume(
    State(state): State<AppState>,
    Path(timeframe): Path<String>,
) -> Result<Json<VolumeEnvelope>, ApiError> {
    let tf = AnalyticsTimeframe::from_str(&timeframe);
    let data = state.volume.fetch(tf).await?;
    Ok(Json(VolumeEnvelope { data }))
}
