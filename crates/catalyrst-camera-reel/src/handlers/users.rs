use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::dto::{
    GalleryImage, GetGalleryImagesResponse, GetImagesResponse, Image, UserDataResponse,
};
use crate::handlers::{default_compact, default_limit, default_offset, optional_auth, MAX_LIMIT};
use crate::http::ApiError;
use crate::AppState;

#[derive(Deserialize, Debug)]
pub struct GetImagesQuery {
    #[serde(default = "default_offset")]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default = "default_compact")]
    pub compact: bool,
}

fn only_public_for(headers: &HeaderMap, method: &str, path: &str, user_address: &str) -> bool {
    !matches!(optional_auth(headers, method, path), Some(signer) if signer.eq_ignore_ascii_case(user_address))
}

pub async fn get_user_data(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(user_address): Path<String>,
) -> Result<Response, ApiError> {
    let only_public = only_public_for(&headers, "get", uri.path(), &user_address);

    let images_count = state
        .db
        .get_user_images_count(&user_address, only_public)
        .await
        .map_err(|_| ApiError::NotFound("user not found".to_string()))?;

    Ok((
        StatusCode::OK,
        Json(UserDataResponse {
            current_images: images_count,
            max_images: state.config.max_images_per_user,
        }),
    )
        .into_response())
}

pub async fn get_user_images(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Path(user_address): Path<String>,
    Query(q): Query<GetImagesQuery>,
) -> Result<Response, ApiError> {
    let only_public = only_public_for(&headers, "get", uri.path(), &user_address);

    let images_count = state
        .db
        .get_user_images_count(&user_address, only_public)
        .await
        .map_err(|_| ApiError::NotFound("user not found".to_string()))?;

    let limit = q.limit.min(MAX_LIMIT) as i64;
    let images = state
        .db
        .get_user_images(&user_address, q.offset as i64, limit, only_public)
        .await
        .map_err(|_| ApiError::NotFound("user not found".to_string()))?;

    let user_data = UserDataResponse {
        current_images: images_count,
        max_images: state.config.max_images_per_user,
    };

    if q.compact {
        let images = images.into_iter().map(GalleryImage::from).collect();
        Ok((
            StatusCode::OK,
            Json(GetGalleryImagesResponse { images, user_data }),
        )
            .into_response())
    } else {
        let images = images.into_iter().map(Image::from).collect();
        Ok((
            StatusCode::OK,
            Json(GetImagesResponse { images, user_data }),
        )
            .into_response())
    }
}
