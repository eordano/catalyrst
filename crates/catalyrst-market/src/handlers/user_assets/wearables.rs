use axum::extract::{Path, Query, State};
use axum::Json;

use super::{create_paginated_response, AssetsHttpResponse};
use crate::http::response::ApiError;
use crate::ports::user_assets::{
    parse_user_assets_params, GroupedWearable, ProfileWearable, UrnToken,
};
use crate::AppState;

pub async fn get_user_wearables(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<ProfileWearable>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data, total, total_items) = state
        .user_assets
        .get_wearables_by_owner(&owner, filters.first, filters.skip)
        .await?;
    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        Some(total_items),
    )))
}

pub async fn get_user_wearables_urn_token(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<UrnToken>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data, total) = state
        .user_assets
        .get_owned_wearables_urn_and_token_id(&owner, filters.first, filters.skip)
        .await?;
    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        None,
    )))
}

pub async fn get_user_grouped_wearables(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<GroupedWearable>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data, total) = state
        .user_assets
        .get_grouped_wearables_by_owner(&owner, &filters)
        .await?;
    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        None,
    )))
}
