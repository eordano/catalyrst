use axum::{
    extract::{Path, State},
    http::{HeaderMap, Uri},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use super::drafts::signer;
use crate::{
    http::{errors::ApiError, response::ApiData},
    ports::{publication_chain::PublicationChain, publication_store::PublicationState},
    AppState,
};

#[derive(Deserialize)]
pub struct BeginPublication {
    revision: String,
}
#[derive(Deserialize)]
pub struct PublicationTransaction {
    tx_hash: String,
}

fn rpc_url(state: &AppState) -> Result<&str, ApiError> {
    state.polygon_rpc_url.as_deref().ok_or_else(|| {
        ApiError::service_unavailable(
            "Collection publication is unavailable until the Polygon verifier is configured",
        )
    })
}

pub async fn begin(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<BeginPublication>,
) -> Result<Json<ApiData<PublicationState>>, ApiError> {
    let owner = signer(&headers, "post", &uri).await?;
    rpc_url(&state)?;
    Ok(Json(ApiData::ok(
        state
            .items
            .begin_publication(owner.as_str(), id, &body.revision)
            .await?,
    )))
}

pub async fn status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<Option<PublicationState>>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state.items.publication_state(owner.as_str(), id).await?,
    )))
}

pub async fn transaction(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<PublicationTransaction>,
) -> Result<Json<ApiData<PublicationState>>, ApiError> {
    let owner = signer(&headers, "put", &uri).await?;
    let chain = PublicationChain {
        http: &state.http,
        url: rpc_url(&state)?,
    };
    Ok(Json(ApiData::ok(
        chain
            .verify(&state.items, owner.as_str(), id, &body.tx_hash)
            .await?,
    )))
}

pub async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<bool>>, ApiError> {
    let owner = signer(&headers, "delete", &uri).await?;
    state.items.cancel_publication(owner.as_str(), id).await?;
    Ok(Json(ApiData::ok(true)))
}

pub async fn claim(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<BeginPublication>,
) -> Result<Json<ApiData<bool>>, ApiError> {
    let owner = signer(&headers, "patch", &uri).await?;
    rpc_url(&state)?;
    state
        .items
        .claim_publication(owner.as_str(), id, &body.revision)
        .await?;
    Ok(Json(ApiData::ok(true)))
}
