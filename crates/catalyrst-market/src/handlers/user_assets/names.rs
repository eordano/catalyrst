use axum::extract::{Path, Query, State};
use axum::Json;

use super::{create_paginated_response, AssetsHttpResponse};
use crate::http::response::ApiError;
use crate::ports::user_assets::{parse_user_assets_params, NameOnly, ProfileName};
use crate::AppState;

pub async fn get_user_names(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<ProfileName>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data, total) = state
        .user_assets
        .get_names_by_owner(&owner, &filters)
        .await?;
    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        None,
    )))
}

pub async fn get_user_names_only(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<NameOnly>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data, total) = state
        .user_assets
        .get_owned_names_only(&owner, filters.first, filters.skip)
        .await?;
    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        None,
    )))
}
