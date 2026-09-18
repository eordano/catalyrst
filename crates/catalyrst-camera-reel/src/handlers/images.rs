use std::io::Cursor;

use axum::body::Body;
use axum::extract::{Multipart, OriginalUri, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;
use bytes::Bytes;
use image::guess_format;
use uuid::Uuid;

use crate::admin::authorize_admin;
use crate::dto::{
    Image, Metadata, UpdateReview, UpdateVisibility, UploadResponse, UserDataResponse,
};
use crate::handlers::require_auth;
use crate::http::ApiError;
use crate::ports::db::{hash_of_url, DeletedImage};
use crate::ports::storage::ImageStore;
use crate::AppState;

pub async fn upload_image(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let address = require_auth(&headers, "post", uri.path()).await?;

    let mut image_bytes: Option<Bytes> = None;
    let mut image_content_type: Option<String> = None;
    let mut metadata_bytes: Option<Bytes> = None;
    let mut is_public = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("invalid multipart: {e}")))?
    {
        match field.name() {
            Some("image") => {
                image_content_type = field.content_type().map(|s| s.to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("invalid image field: {e}")))?;
                if data.len() > 5 * 1024 * 1024 {
                    return Err(ApiError::BadRequest("image too large".to_string()));
                }
                image_bytes = Some(data);
            }
            Some("metadata") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("invalid metadata field: {e}")))?;
                metadata_bytes = Some(data);
            }
            Some("is_public") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("invalid is_public field: {e}")))?;
                is_public = text.trim().parse::<bool>().unwrap_or(false);
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    let image_bytes =
        image_bytes.ok_or_else(|| ApiError::BadRequest("missing image".to_string()))?;
    let metadata_bytes =
        metadata_bytes.ok_or_else(|| ApiError::BadRequest("missing metadata".to_string()))?;

    let metadata: Metadata = serde_json::from_slice(&metadata_bytes).map_err(|e| {
        tracing::error!("failed to parse metadata: {e}");
        ApiError::BadRequest("invalid metadata".to_string())
    })?;

    finalize_upload(
        &state,
        address.as_str(),
        image_bytes,
        image_content_type,
        metadata,
        is_public,
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct JsonUpload {
    pub image: String,
    pub content_type: String,
    pub metadata: Metadata,
    #[serde(default)]
    pub is_public: bool,
}

pub async fn upload_image_json(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(req): Json<JsonUpload>,
) -> Result<Response, ApiError> {
    let address = require_auth(&headers, "post", uri.path()).await?;

    let image_bytes = Bytes::from(
        base64::engine::general_purpose::STANDARD
            .decode(req.image.trim())
            .map_err(|_| ApiError::BadRequest("invalid base64 image".to_string()))?,
    );
    if image_bytes.len() > 15 * 1024 * 1024 {
        return Err(ApiError::BadRequest("image too large".to_string()));
    }

    finalize_upload(
        &state,
        address.as_str(),
        image_bytes,
        Some(req.content_type),
        req.metadata,
        req.is_public,
    )
    .await
}

async fn finalize_upload(
    state: &AppState,
    address: &str,
    image_bytes: Bytes,
    image_content_type: Option<String>,
    metadata: Metadata,
    is_public: bool,
) -> Result<Response, ApiError> {
    if !metadata.user_address.eq_ignore_ascii_case(address) {
        return Err(ApiError::BadRequest("invalid user address".to_string()));
    }

    let content_type = image_content_type
        .ok_or_else(|| ApiError::BadRequest("invalid content type".to_string()))?;
    match content_type.as_str() {
        "image/png" | "image/jpeg" => {}
        _ => return Err(ApiError::BadRequest("unsupported content type".to_string())),
    }

    let format = guess_format(&image_bytes)
        .map_err(|_| ApiError::BadRequest("invalid image format".to_string()))?;

    let thumbnail = {
        let img = image::load_from_memory_with_format(&image_bytes, format).map_err(|e| {
            tracing::error!("failed to parse image: {e}");
            ApiError::BadRequest("invalid image".to_string())
        })?;
        let thumb = img.thumbnail(640, 360);
        let mut buffer = Cursor::new(Vec::new());
        thumb.write_to(&mut buffer, format).map_err(|e| {
            tracing::error!("couldn't generate thumbnail: {e}");
            ApiError::BadRequest("couldn't create thumbnail".to_string())
        })?;
        Bytes::from(buffer.into_inner())
    };

    let image_hash = ImageStore::hash(&image_bytes);
    let thumbnail_hash = ImageStore::hash(&thumbnail);

    let image_id = Uuid::new_v4().to_string();
    let api_url = &state.config.api_url;
    let image = Image {
        id: image_id.clone(),
        url: format!("{api_url}/api/images/{image_hash}"),
        thumbnail_url: format!("{api_url}/api/images/{thumbnail_hash}"),
        is_public,
        metadata,
    };

    // The row is the limit gate, so it lands before the blobs: a refused upload
    // never touches the disk, and a failed blob write takes its row back out.
    let max_images = state.config.max_images_per_user;
    let current_images = state
        .db
        .insert_image_within_limit(&image, max_images)
        .await
        .map_err(|e| {
            tracing::error!("failed to store image metadata: {e}");
            ApiError::Internal("failed to store image metadata".to_string())
        })?
        .ok_or_else(|| {
            ApiError::MaxLimitReached(format!(
                "you have reached the limit of {max_images} max images"
            ))
        })?;

    let stored = async {
        state
            .store
            .store_as(&image_hash, image_bytes)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to store image: {e}")))?;
        state
            .store
            .store_as(&thumbnail_hash, thumbnail)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to store thumbnail: {e}")))
    }
    .await;
    if let Err(err) = stored {
        let _ = state.db.delete_image(&image_id).await;
        return Err(err);
    }

    let response = UploadResponse {
        image,
        user_data: UserDataResponse {
            current_images,
            max_images,
        },
    };
    Ok((StatusCode::OK, Json(response)).into_response())
}

pub async fn delete_image(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(image_id): Path<String>,
) -> Result<Response, ApiError> {
    let address = require_auth(&headers, "delete", uri.path()).await?;

    let deleted = delete_image_row(&state, &image_id, Some(address.as_str())).await?;
    if !deleted.deleted {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }
    delete_unreferenced_blobs(&state, &deleted).await;

    Ok((
        StatusCode::OK,
        Json(UserDataResponse {
            current_images: deleted.remaining,
            max_images: state.config.max_images_per_user,
        }),
    )
        .into_response())
}

async fn delete_image_row(
    state: &AppState,
    image_id: &str,
    owner: Option<&str>,
) -> Result<DeletedImage, ApiError> {
    state
        .db
        .delete_image_owned(image_id, owner)
        .await
        .map_err(|e| {
            tracing::error!("failed to delete image metadata: {e}");
            ApiError::Internal("failed to delete image".to_string())
        })?
        .ok_or_else(|| ApiError::NotFound("image not found".to_string()))
}

/// Blobs are content-addressed with no refcount, and any user can re-upload another user's
/// public bytes to the same hash, so an unconditional unlink would 404 every other live row
/// sharing that blob. The delete statement already reported which hashes other rows still
/// reference.
async fn delete_unreferenced_blobs(state: &AppState, deleted: &DeletedImage) {
    for (url, shared) in [
        (deleted.url.as_str(), deleted.image_shared),
        (deleted.thumbnail_url.as_str(), deleted.thumbnail_shared),
    ] {
        if !shared {
            let _ = state.store.delete(hash_of_url(url)).await;
        }
    }
}

pub async fn update_image_visibility(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(image_id): Path<String>,
    Json(update): Json<UpdateVisibility>,
) -> Result<Response, ApiError> {
    let address = require_auth(&headers, "patch", uri.path()).await?;

    let owned = state
        .db
        .set_image_visibility(&image_id, address.as_str(), update.is_public)
        .await
        .map_err(|e| {
            tracing::error!("failed to update image metadata: {e}");
            ApiError::Internal("failed to update image metadata".to_string())
        })?
        .ok_or_else(|| ApiError::NotFound("image not found".to_string()))?;
    if !owned {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }

    Ok(StatusCode::OK.into_response())
}

pub async fn get_image(
    State(state): State<AppState>,
    Path(image_id): Path<String>,
) -> Result<Response, ApiError> {
    let servable = state.db.hash_is_servable(&image_id).await.map_err(|e| {
        tracing::error!("failed to check image review status: {e}");
        ApiError::NotFound("image not found".to_string())
    })?;
    if !servable {
        return Err(ApiError::NotFound("image not found".to_string()));
    }

    if let Some(bucket_url) = &state.config.bucket_url {
        let location = format!("{}/{}", bucket_url.trim_end_matches('/'), image_id);
        return Ok((
            StatusCode::TEMPORARY_REDIRECT,
            [(header::LOCATION, location)],
        )
            .into_response());
    }

    let bytes = state
        .store
        .retrieve(&image_id)
        .await
        .map_err(|e| match e {
            catalyrst_storage::StorageError::InvalidId(_)
            | catalyrst_storage::StorageError::PathTraversal(_) => {
                ApiError::BadRequest(format!("invalid image id: {e}"))
            }
            other => ApiError::Internal(format!("failed to read image: {other}")),
        })?
        .ok_or_else(|| ApiError::NotFound("image not found".to_string()))?;

    let content_type = match guess_format(&bytes) {
        Ok(image::ImageFormat::Png) => "image/png",
        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
        _ => "application/octet-stream",
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        Body::from(bytes),
    )
        .into_response())
}

pub async fn admin_delete_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(image_id): Path<String>,
) -> Result<Response, ApiError> {
    authorize_admin(&state, &headers)?;

    let deleted = delete_image_row(&state, &image_id, None).await?;
    delete_unreferenced_blobs(&state, &deleted).await;

    Ok((
        StatusCode::OK,
        Json(UserDataResponse {
            current_images: deleted.remaining,
            max_images: state.config.max_images_per_user,
        }),
    )
        .into_response())
}

pub async fn admin_update_image_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(image_id): Path<String>,
    Json(update): Json<UpdateReview>,
) -> Result<Response, ApiError> {
    authorize_admin(&state, &headers)?;

    if !update.is_valid() {
        return Err(ApiError::BadRequest(
            "reviewStatus must be one of: ok, flagged, rejected".to_string(),
        ));
    }

    let affected = state
        .db
        .update_image_review_status(&image_id, &update.review_status)
        .await
        .map_err(|e| {
            tracing::error!("failed to update image review status: {e}");
            ApiError::Internal("failed to update image review status".to_string())
        })?;

    if affected == 0 {
        return Err(ApiError::NotFound("image not found".to_string()));
    }

    Ok(StatusCode::OK.into_response())
}

pub async fn get_metadata(
    State(state): State<AppState>,
    Path(image_id): Path<String>,
) -> Result<Response, ApiError> {
    let db_image = match state.db.get_image(&image_id).await {
        Ok(img) => img,
        Err(sqlx::Error::ColumnDecode { source, .. }) => {
            tracing::debug!("couldn't decode image metadata: {source:?}");
            return Err(ApiError::Internal("couldn't decode image".to_string()));
        }
        Err(e) => {
            tracing::debug!("image not found: {e:?}");
            return Err(ApiError::NotFound("image not found".to_string()));
        }
    };

    if db_image.review_status == "rejected" {
        return Err(ApiError::NotFound("image not found".to_string()));
    }

    let image: Image = db_image.into();
    Ok((StatusCode::OK, Json(image)).into_response())
}
