//! Direct port of `marketplace-server/src/controllers/handlers/trending-handler.ts`.
//! The handler emits a Cache-Control: public,max-age=3600,s-maxage=3600 header
//! upstream — we leave the response wrapper as plain JSON since axum lets a
//! layer add the header. Header parity with the Node server is part of the
//! crate-level middleware setup (wired in lib.rs), not the per-handler module.

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
