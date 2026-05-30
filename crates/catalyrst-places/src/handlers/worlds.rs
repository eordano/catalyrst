use axum::extract::{Path, Query, State};
use axum::Json;

use crate::http::errors::ApiError;
use crate::http::response::{ApiData, ApiDataTotal};
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, PlaceRow};
use crate::AppState;

pub async fn get_world(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(world_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    match state.places.find_world_by_id(&world_id).await? {
        Some(mut w) => {
            let user = crate::auth::auth_address_optional(&headers);
            state
                .places
                .apply_user_interactions(user.as_deref(), std::slice::from_mut(&mut w))
                .await;
            Ok(Json(ApiData::ok(w)))
        }
        None => Err(ApiError::not_found(format!("Not found world \"{}\"", world_id))),
    }
}

pub async fn get_world_list(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<PlaceRow>>, ApiError> {
    let get = |k: &str| pairs.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
    let get_all = |k: &str| {
        pairs
            .iter()
            .filter(|(p, _)| p == k)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    let user = crate::auth::auth_address_optional(&headers);
    let only_favorites = get("only_favorites")
        .map(|v| matches!(v.as_str(), "true" | "1"))
        .unwrap_or(false);
    let mut favorite_ids: Vec<String> = Vec::new();
    if only_favorites {
        match &user {
            None => return Ok(Json(ApiDataTotal::ok(vec![], 0))),
            Some(addr) => match state.places.favorite_entity_ids(addr).await? {
                Some(ids) if !ids.is_empty() => favorite_ids = ids,
                _ => return Ok(Json(ApiDataTotal::ok(vec![], 0))),
            },
        }
    }
    let limit = get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 100);
    let offset = get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let filters = PlaceListFilters {
        limit,
        offset,
        names: get_all("names"),
        categories: get_all("categories"),
        search: get("search"),
        order_by: PlaceOrderBy::parse(get("order_by").as_deref()),
        order_desc: !matches!(get("order").as_deref(), Some("asc")),
        only_worlds: true,
        ids: favorite_ids,
        creator_address: get("owner").map(|s| s.to_lowercase()),
        ..Default::default()
    };
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    state
        .places
        .apply_user_interactions(user.as_deref(), &mut data)
        .await;
    Ok(Json(ApiDataTotal::ok(data, total)))
}

pub async fn get_world_names_list(
    State(state): State<AppState>,
) -> Result<Json<ApiDataTotal<String>>, ApiError> {
    let names = state.places.world_names().await?;
    let total = names.len() as i64;
    Ok(Json(ApiDataTotal::ok(names, total)))
}
