use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::dto::{
    GalleryImage, GalleryImageWithPlace, GetMultiplePlacesImagesBody,
    GetMultiplePlacesImagesResponse, GetPlaceImagesResponse, PlaceDataResponse,
};
use crate::handlers::{default_limit, default_offset, MAX_LIMIT};
use crate::http::ApiError;
use crate::AppState;

const MAX_PLACES_IDS: usize = 100;

#[derive(Deserialize, Debug)]
pub struct PageQuery {
    #[serde(default = "default_offset")]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn capped_limit(limit: u64) -> i64 {
    limit.min(MAX_LIMIT) as i64
}

fn validate_places_ids(places_ids: &[String]) -> Result<(), ApiError> {
    if places_ids.is_empty() {
        return Err(ApiError::BadRequest("no place IDs provided".to_string()));
    }
    if places_ids.len() > MAX_PLACES_IDS {
        return Err(ApiError::BadRequest(format!(
            "too many place IDs provided, maximum is {MAX_PLACES_IDS}"
        )));
    }
    Ok(())
}

pub async fn get_place_images(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
    Query(q): Query<PageQuery>,
) -> Result<Response, ApiError> {
    let limit = capped_limit(q.limit);
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
            .get_multiple_places_images(&place_ids, q.offset as i64, limit)
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
        .get_place_images(&place_id, q.offset as i64, limit)
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
    validate_places_ids(&body.places_ids)?;

    let limit = capped_limit(q.limit);
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
        .get_multiple_places_images(&resolved_ids, q.offset as i64, limit)
        .await
        .map_err(|_| ApiError::NotFound("places not found".to_string()))?;

    let images = images
        .into_iter()
        .map(GalleryImageWithPlace::from)
        .collect();
    Ok((
        StatusCode::OK,
        Json(GetMultiplePlacesImagesResponse {
            images,
            place_data: PlaceDataResponse { max_images: count },
        }),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::{capped_limit, validate_places_ids, MAX_PLACES_IDS};
    use crate::handlers::MAX_LIMIT;
    use crate::http::ApiError;

    #[test]
    fn capped_limit_passes_values_at_or_below_max() {
        assert_eq!(capped_limit(0), 0);
        assert_eq!(capped_limit(20), 20);
        assert_eq!(capped_limit(MAX_LIMIT), MAX_LIMIT as i64);
    }

    #[test]
    fn capped_limit_clamps_values_above_max() {
        assert_eq!(capped_limit(MAX_LIMIT + 1), MAX_LIMIT as i64);
        assert_eq!(capped_limit(1_000), MAX_LIMIT as i64);
        assert_eq!(capped_limit(u64::MAX), MAX_LIMIT as i64);
    }

    #[test]
    fn validate_places_ids_rejects_empty() {
        let err = validate_places_ids(&[]).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn validate_places_ids_accepts_up_to_max() {
        let ids: Vec<String> = (0..MAX_PLACES_IDS).map(|i| i.to_string()).collect();
        assert_eq!(ids.len(), MAX_PLACES_IDS);
        assert!(validate_places_ids(&ids).is_ok());
    }

    #[test]
    fn validate_places_ids_rejects_over_max() {
        let ids: Vec<String> = (0..=MAX_PLACES_IDS).map(|i| i.to_string()).collect();
        assert_eq!(ids.len(), MAX_PLACES_IDS + 1);
        let err = validate_places_ids(&ids).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}
