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
use crate::AppState;

pub async fn upload_image(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let address = require_auth(&headers, "post", uri.path())?;

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
        &address,
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
    let address = require_auth(&headers, "post", uri.path())?;

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
        &address,
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
    let images_count = state
        .db
        .get_user_images_count(address, false)
        .await
        .unwrap_or(0);
    if images_count >= state.config.max_images_per_user {
        let message = format!(
            "you have reached the limit of {} max images",
            state.config.max_images_per_user
        );
        return Err(ApiError::MaxLimitReached(message));
    }

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

    let image_hash = state
        .store
        .store(image_bytes)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to store image: {e}")))?;
    let thumbnail_hash = state
        .store
        .store(thumbnail)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to store thumbnail: {e}")))?;

    let image_id = Uuid::new_v4().to_string();
    let api_url = &state.config.api_url;
    let image = Image {
        id: image_id.clone(),
        url: format!("{api_url}/api/images/{image_hash}"),
        thumbnail_url: format!("{api_url}/api/images/{thumbnail_hash}"),
        is_public,
        metadata,
    };

    state.db.insert_image(&image).await.map_err(|e| {
        tracing::error!("failed to store image metadata: {e}");
        ApiError::Internal("failed to store image metadata".to_string())
    })?;

    let response = UploadResponse {
        image,
        user_data: UserDataResponse {
            current_images: images_count + 1,
            max_images: state.config.max_images_per_user,
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
    let address = require_auth(&headers, "delete", uri.path())?;

    let image = state
        .db
        .get_image(&image_id)
        .await
        .map_err(|_| ApiError::NotFound("image not found".to_string()))?;

    if !image.user_address.eq_ignore_ascii_case(&address) {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }

    state.db.delete_image(&image_id).await.map_err(|e| {
        tracing::error!("failed to delete image metadata: {e}");
        ApiError::Internal("failed to delete image".to_string())
    })?;

    if let Some(hash) = image.url.rsplit('/').next() {
        let _ = state.store.delete(hash).await;
    }
    if let Some(hash) = image.thumbnail_url.rsplit('/').next() {
        let _ = state.store.delete(hash).await;
    }

    let current_images = state
        .db
        .get_user_images_count(&image.user_address, false)
        .await
        .unwrap_or(0);

    Ok((
        StatusCode::OK,
        Json(UserDataResponse {
            current_images,
            max_images: state.config.max_images_per_user,
        }),
    )
        .into_response())
}

pub async fn update_image_visibility(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(image_id): Path<String>,
    Json(update): Json<UpdateVisibility>,
) -> Result<Response, ApiError> {
    let address = require_auth(&headers, "patch", uri.path())?;

    let image = state
        .db
        .get_image(&image_id)
        .await
        .map_err(|_| ApiError::NotFound("image not found".to_string()))?;

    if !image.user_address.eq_ignore_ascii_case(&address) {
        return Err(ApiError::Forbidden("forbidden".to_string()));
    }

    if image.is_public == update.is_public {
        return Ok(StatusCode::OK.into_response());
    }

    state
        .db
        .update_image_visibility(&image_id, update.is_public)
        .await
        .map_err(|e| {
            tracing::error!("failed to update image metadata: {e}");
            ApiError::Internal("failed to update image metadata".to_string())
        })?;

    Ok(StatusCode::OK.into_response())
}

pub async fn get_image(
    State(state): State<AppState>,
    Path(image_id): Path<String>,
) -> Result<Response, ApiError> {
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

    let image = state
        .db
        .get_image(&image_id)
        .await
        .map_err(|_| ApiError::NotFound("image not found".to_string()))?;

    state.db.delete_image(&image_id).await.map_err(|e| {
        tracing::error!("failed to delete image metadata: {e}");
        ApiError::Internal("failed to delete image".to_string())
    })?;

    if let Some(hash) = image.url.rsplit('/').next() {
        let _ = state.store.delete(hash).await;
    }
    if let Some(hash) = image.thumbnail_url.rsplit('/').next() {
        let _ = state.store.delete(hash).await;
    }

    let current_images = state
        .db
        .get_user_images_count(&image.user_address, false)
        .await
        .unwrap_or(0);

    Ok((
        StatusCode::OK,
        Json(UserDataResponse {
            current_images,
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

    let image: Image = db_image.into();
    Ok((StatusCode::OK, Json(image)).into_response())
}
