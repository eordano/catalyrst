//! Direct port of `marketplace-server/src/controllers/handlers/items-handler.ts`.
//!
//! Endpoint: `GET /v1/items`.
//!
//! The TS handler enriches the response with `picks.getPicksStats(...)` via
//! the favorites component; favorites is out of scope per
//! `AGENT-PORT-INSTRUCTIONS.md`, so the `picks` field stays at its default
//! `{count: 0}` value — exactly the value `fromDBItemToItem` already returns
//! before enrichment. Re-enable by wiring through the picks component once
//! the favorites federation ADR is in.

use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::items::{parse_filters, Item};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ItemsResponseBody {
    pub data: Vec<Item>,
    pub total: i64,
}

pub async fn get_items(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ItemsResponseBody>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.items.get_items(&filters).await?;
    Ok(Json(ItemsResponseBody { data, total }))
}
