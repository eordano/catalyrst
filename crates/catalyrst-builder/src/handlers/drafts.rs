use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::http::{errors::ApiError, response::ApiData};
use crate::ports::drafts::{CollectionDraft, CollectionListingOut, ItemDraft};
use crate::ports::items::CollectionMetaOut;
use crate::AppState;

pub(super) async fn signer(
    headers: &HeaderMap,
    method: &str,
    uri: &Uri,
) -> Result<catalyrst_crypto::Signer, ApiError> {
    require_signer(headers, method, uri.path())
        .await
        .map_err(|error| {
            let (status, message) = error.http_status_and_message();
            ApiError::http(status, message)
        })
}

#[derive(Deserialize)]
pub struct CollectionBody {
    collection: CollectionDraft,
}
#[derive(Deserialize)]
pub struct ItemBody {
    item: ItemDraft,
}

pub async fn put_collection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<CollectionBody>,
) -> Result<Json<ApiData<CollectionMetaOut>>, ApiError> {
    let owner = signer(&headers, "put", &uri).await?;
    body.collection.validate(id, owner.as_str())?;
    Ok(Json(ApiData::ok(
        state
            .items
            .save_collection_draft(owner.as_str(), &body.collection)
            .await?,
    )))
}

pub async fn get_drafts(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<Vec<CollectionListingOut>>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state.items.collection_drafts(owner.as_str()).await?,
    )))
}

pub async fn get_item_drafts(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<Vec<Value>>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state.items.item_drafts(owner.as_str()).await?,
    )))
}

pub async fn put_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<ItemBody>,
) -> Result<Json<ApiData<Value>>, ApiError> {
    let owner = signer(&headers, "put", &uri).await?;
    body.item.validate(id, owner.as_str())?;
    Ok(Json(ApiData::ok(
        state
            .items
            .save_item_draft(owner.as_str(), &body.item)
            .await?,
    )))
}

pub async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<Value>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state.items.item_draft(owner.as_str(), id).await?,
    )))
}

pub async fn post_item_files(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<ApiData<std::collections::BTreeMap<String, String>>>, ApiError> {
    let owner = signer(&headers, "post", &uri).await?;
    state.items.item_draft(owner.as_str(), id).await?;
    let mut files = Vec::new();
    let mut size = 0;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("Invalid multipart upload"))?
    {
        let name = field
            .file_name()
            .ok_or_else(|| ApiError::bad_request("Filename is required"))?
            .to_owned();
        if !crate::ports::drafts::valid_filename(&name) || files.len() >= 100 {
            return Err(ApiError::bad_request("Invalid filename or too many files"));
        }
        let bytes = field
            .bytes()
            .await
            .map_err(|_| ApiError::bad_request("File upload exceeds 20 MB or is incomplete"))?;
        size += bytes.len();
        if size > 20 * 1024 * 1024 {
            return Err(ApiError::bad_request("Upload exceeds 20 MB"));
        }
        files.push((name, bytes.to_vec()));
    }
    Ok(Json(ApiData::ok(
        state
            .items
            .save_item_files(owner.as_str(), id, files)
            .await?,
    )))
}

pub async fn get_publication(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiData<crate::ports::publication::PublicationPreparation>>, ApiError> {
    let owner = signer(&headers, "get", &uri).await?;
    Ok(Json(ApiData::ok(
        state.items.prepare_publication(owner.as_str(), id).await?,
    )))
}
