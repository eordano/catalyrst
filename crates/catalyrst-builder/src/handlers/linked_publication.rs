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
    ports::{
        linked_publication::LinkedPublicationCheque,
        linked_publication_store::{LinkedPublicationPreparation, LinkedPublicationState},
    },
    AppState,
};

#[derive(Deserialize)]
pub struct Revision {
    revision: String,
}

pub async fn prepare(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<LinkedPublicationPreparation>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state
            .items
            .prepare_linked_publication(owner.as_str(), id)
            .await?,
    )))
}

pub async fn status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<Option<LinkedPublicationState>>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state
            .items
            .linked_publication_state(owner.as_str(), id)
            .await?,
    )))
}

pub async fn begin(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Revision>,
) -> Result<Json<ApiData<LinkedPublicationState>>, ApiError> {
    let owner = signer(&headers, "post", &uri).await?;
    Ok(Json(ApiData::ok(
        state
            .items
            .begin_linked_publication(owner.as_str(), id, &body.revision)
            .await?,
    )))
}

pub async fn claim(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Revision>,
) -> Result<Json<ApiData<LinkedPublicationState>>, ApiError> {
    let owner = signer(&headers, "patch", &uri).await?;
    Ok(Json(ApiData::ok(
        state
            .items
            .claim_linked_publication(owner.as_str(), id, &body.revision)
            .await?,
    )))
}

pub async fn authorize(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<LinkedPublicationCheque>,
) -> Result<Json<ApiData<LinkedPublicationState>>, ApiError> {
    let owner = signer(&headers, "put", &uri).await?;
    Ok(Json(ApiData::ok(
        state
            .items
            .authorize_linked_publication(owner.as_str(), id, &body)
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
    state
        .items
        .cancel_linked_publication(owner.as_str(), id)
        .await?;
    Ok(Json(ApiData::ok(true)))
}

pub async fn verify(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<crate::ports::linked_publication_review::FoundationReviewSignatures>,
) -> Result<Json<ApiData<LinkedPublicationState>>, ApiError> {
    let owner = signer(&headers, "post", &uri).await?;
    let verifier = crate::ports::linked_publication_review::FoundationLinkedReview::new()?;
    Ok(Json(ApiData::ok(
        verifier
            .verify(&state.items, owner.as_str(), id, &body)
            .await?,
    )))
}

pub async fn private_response(mut response: axum::response::Response) -> axum::response::Response {
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    response
}
