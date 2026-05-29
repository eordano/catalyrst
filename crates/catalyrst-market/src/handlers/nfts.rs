//! Direct port of `marketplace-server/src/controllers/handlers/nfts-handler.ts`.
//!
//! Endpoint: `GET /v1/nfts`.
//!
//! The `verification` middleware in the TS source only feeds an optional
//! caller address into rental-listings lookup; the read path doesn't filter
//! on it, so we accept the request without it. Authn/Z is enforced at the
//! reverse proxy in catalyrst.

use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::nfts::{parse_filters, NftResult};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct NftsResponseBody {
    pub data: Vec<NftResult>,
    pub total: i64,
}

pub async fn get_nfts(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<NftsResponseBody>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.nfts.get_nfts(&filters, None).await?;
    Ok(Json(NftsResponseBody { data, total }))
}
