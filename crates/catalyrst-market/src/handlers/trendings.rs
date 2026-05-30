use axum::extract::{Query, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::items::Item;
use crate::ports::trendings::parse_filters;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct TrendingsEnvelope {
    pub data: Vec<Item>,
}

pub async fn get_trendings(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<(HeaderMap, Json<TrendingsEnvelope>), ApiError> {
    let filters = parse_filters(&pairs)?;
    let data = state.trendings.fetch(&state.items, &filters).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        "public,max-age=3600,s-maxage=3600".parse().unwrap(),
    );
    Ok((headers, Json(TrendingsEnvelope { data })))
}
