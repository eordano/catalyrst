use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::dto::{
    GalleryImage, GalleryImageWithPlace, GetMultiplePlacesImagesBody,
    GetMultiplePlacesImagesResponse, GetPlaceImagesResponse, PlaceDataResponse,
};
use crate::handlers::{default_limit, default_offset};
use crate::http::ApiError;
use crate::AppState;

#[derive(Deserialize, Debug)]
pub struct PageQuery {
    #[serde(default = "default_offset")]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

pub async fn get_place_images(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
    Query(q): Query<PageQuery>,
) -> Result<Response, ApiError> {
    if place_id.ends_with(".eth") {
        let place_ids = state
            .places
            .get_world_place_ids(&place_id)
            .await
            .map_err(|e| {
                tracing::error!("failed to resolve world name '{place_id}': {e}");
                ApiError::BadGateway(format!("failed to resolve world name: {e}"))
            })?;

        if place_ids.is_empty() {
            return Ok((
                StatusCode::OK,
                Json(GetPlaceImagesResponse {
                    images: vec![],
                    place_data: PlaceDataResponse { max_images: 0 },
                }),
            )
                .into_response());
        }

        let count = state
            .db
            .get_multiple_places_images_count(&place_ids)
            .await
            .map_err(|_| ApiError::NotFound("place not found".to_string()))?;
        let images = state
            .db
            .get_multiple_places_images(&place_ids, q.offset as i64, q.limit as i64)
            .await
            .map_err(|_| ApiError::NotFound("place not found".to_string()))?;

        let images = images.into_iter().map(GalleryImage::from).collect();
        return Ok((
            StatusCode::OK,
            Json(GetPlaceImagesResponse {
                images,
                place_data: PlaceDataResponse { max_images: count },
            }),
        )
            .into_response());
    }

    let count = state
        .db
        .get_place_images_count(&place_id)
        .await
        .map_err(|_| ApiError::NotFound("place not found".to_string()))?;
    let images = state
        .db
        .get_place_images(&place_id, q.offset as i64, q.limit as i64)
        .await
        .map_err(|_| ApiError::NotFound("place not found".to_string()))?;

    let images = images.into_iter().map(GalleryImage::from).collect();
    Ok((
        StatusCode::OK,
        Json(GetPlaceImagesResponse {
            images,
            place_data: PlaceDataResponse { max_images: count },
        }),
    )
        .into_response())
}

pub async fn get_multiple_places_images(
    State(state): State<AppState>,
    Query(q): Query<PageQuery>,
    Json(body): Json<GetMultiplePlacesImagesBody>,
) -> Result<Response, ApiError> {
    if body.places_ids.is_empty() {
        return Err(ApiError::BadRequest("no place IDs provided".to_string()));
    }

    let mut resolved_ids: Vec<String> = Vec::new();
    for id in body.places_ids {
        if id.ends_with(".eth") {
            let ids = state.places.get_world_place_ids(&id).await.map_err(|e| {
                tracing::error!("failed to resolve world name '{id}': {e}");
                ApiError::BadGateway(format!("failed to resolve world name: {e}"))
            })?;
            resolved_ids.extend(ids);
        } else {
            resolved_ids.push(id);
        }
    }

    if resolved_ids.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(GetMultiplePlacesImagesResponse {
                images: vec![],
                place_data: PlaceDataResponse { max_images: 0 },
            }),
        )
            .into_response());
    }

    let count = state
        .db
        .get_multiple_places_images_count(&resolved_ids)
        .await
        .map_err(|_| ApiError::NotFound("places not found".to_string()))?;
    let images = state
        .db
        .get_multiple_places_images(&resolved_ids, q.offset as i64, q.limit as i64)
        .await
        .map_err(|_| ApiError::NotFound("places not found".to_string()))?;

    let images = images.into_iter().map(GalleryImageWithPlace::from).collect();
    Ok((
        StatusCode::OK,
        Json(GetMultiplePlacesImagesResponse {
            images,
            place_data: PlaceDataResponse { max_images: count },
        }),
    )
        .into_response())
}
