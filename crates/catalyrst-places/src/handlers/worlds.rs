use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::Method;
use axum::Json;

use crate::http::errors::ApiError;
use crate::http::response::{ApiData, ApiDataTotal};
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, WorldRow};
use crate::AppState;

#[utoipa::path(
    get,
    path = "/worlds/{world_id}",
    tag = "worlds",
    params(("world_id" = String, Path)),
    responses(
        (status = 200, body = ApiData<WorldRow>),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_world(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: axum::http::HeaderMap,
    Path(world_id): Path<String>,
) -> Result<Json<ApiData<WorldRow>>, ApiError> {
    let user = crate::auth::auth_address_optional(&headers, method.as_str(), uri.path()).await;
    match state
        .places
        .find_world_by_id_for(&world_id, user.as_deref())
        .await?
    {
        Some(w) => Ok(Json(ApiData::ok(WorldRow::from(w)))),
        None => Err(ApiError::not_found(format!(
            "Not found world \"{}\"",
            world_id
        ))),
    }
}

#[utoipa::path(
    get,
    path = "/worlds",
    tag = "worlds",
    params(("limit" = Option<i64>, Query),
        ("offset" = Option<i64>, Query),
        ("names" = Option<Vec<String>>, Query),
        ("categories" = Option<Vec<String>>, Query),
        ("only_favorites" = Option<String>, Query),
        ("only_highlighted" = Option<String>, Query),
        ("only_excluded_from_ranking" = Option<String>, Query,
            description = "True to show only entries the automated ranking is not allowed to touch"),
        ("search" = Option<String>, Query),
        ("order_by" = Option<String>, Query),
        ("order" = Option<String>, Query),
        ("owner" = Option<String>, Query)),
    responses(
        (status = 200, body = ApiDataTotal<WorldRow>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_world_list(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: axum::http::HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<WorldRow>>, ApiError> {
    let get = |k: &str| pairs.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
    let get_all = |k: &str| {
        pairs
            .iter()
            .filter(|(p, _)| p == k)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    let user = crate::auth::auth_address_optional(&headers, method.as_str(), uri.path()).await;
    let bool_q = |k: &str| {
        get(k)
            .map(|v| matches!(v.as_str(), "true" | "1"))
            .unwrap_or(false)
    };
    let only_favorites = bool_q("only_favorites");
    let limit = get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 100);
    let offset = get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let mut filters = PlaceListFilters {
        limit,
        offset,
        names: get_all("names"),
        categories: get_all("categories"),
        search: get("search"),
        order_by: PlaceOrderBy::parse(get("order_by").as_deref()),
        order_desc: !matches!(get("order").as_deref(), Some("asc")),
        only_highlighted: bool_q("only_highlighted"),
        only_excluded_from_ranking: bool_q("only_excluded_from_ranking"),
        only_worlds: true,
        creator_address: get("owner").map(|s| s.to_lowercase()),
        ..Default::default()
    };
    if !state
        .places
        .scope_to_viewer(&mut filters, user.as_deref(), only_favorites)
        .await?
    {
        return Ok(Json(ApiDataTotal::ok(vec![], 0)));
    }
    let (data, total) = state.places.list_page(&filters).await?;
    let worlds: Vec<WorldRow> = data.into_iter().map(WorldRow::from).collect();
    Ok(Json(ApiDataTotal::ok(worlds, total)))
}

#[utoipa::path(
    get,
    path = "/world_names",
    tag = "worlds",
    responses(
        (status = 200, body = ApiDataTotal<String>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_world_names_list(
    State(state): State<AppState>,
) -> Result<Json<ApiDataTotal<String>>, ApiError> {
    let names = state.places.world_names().await?;
    let total = names.len() as i64;
    Ok(Json(ApiDataTotal::ok(names, total)))
}
