use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::http::errors::ApiError;
use crate::http::response::{ApiDataTotal, ApiDataTotalMap};
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, PlaceRow};
use crate::AppState;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct MapEntry {
    pub id: String,
    pub base_position: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub contact_name: Option<String>,
    pub categories: Vec<String>,
    pub user_favorite: bool,
    pub user_like: bool,
    pub user_dislike: bool,
    pub user_visits: i32,
    pub user_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "Array<Record<string, unknown>>"))]
    pub realms_detail: Option<Vec<serde_json::Value>>,
}

impl From<&PlaceRow> for MapEntry {
    fn from(p: &PlaceRow) -> Self {
        Self {
            id: p.id.clone(),
            base_position: p.base_position.clone(),
            title: p.title.clone(),
            description: p.description.clone(),
            image: p.image.clone(),
            contact_name: p.contact_name.clone(),
            categories: p.categories.clone(),
            user_favorite: p.user_favorite,
            user_like: p.user_like,
            user_dislike: p.user_dislike,
            user_visits: p.user_visits,
            user_count: p.user_count.unwrap_or(0),
            realms_detail: p.realms_detail.clone(),
        }
    }
}

fn list_filters(pairs: &[(String, String)], only_worlds: bool) -> (PlaceListFilters, bool) {
    let get = |k: &str| pairs.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
    let get_all = |k: &str| {
        pairs
            .iter()
            .filter(|(p, _)| p == k)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    let only_favorites = get("only_favorites")
        .map(|v| matches!(v.as_str(), "true" | "1"))
        .unwrap_or(false);
    let limit = get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 100);
    let offset = get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let f = PlaceListFilters {
        limit,
        offset,
        positions: get_all("positions"),
        categories: get_all("categories"),
        only_highlighted: get("only_highlighted")
            .map(|v| matches!(v.as_str(), "true" | "1"))
            .unwrap_or(false),
        search: get("search"),
        creator_address: get("creator_address").map(|s| s.to_lowercase()),
        sdk: get("sdk"),
        order_by: PlaceOrderBy::parse(get("order_by").as_deref()),
        order_desc: !matches!(get("order").as_deref(), Some("asc")),
        only_worlds,
        ..Default::default()
    };
    (f, only_favorites)
}

#[utoipa::path(
    get,
    path = "/map",
    tag = "map",
    responses(
        (status = 200, body = ApiDataTotalMap<MapEntry>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_map_places(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotalMap<MapEntry>>, ApiError> {
    let (mut filters, only_favorites) = list_filters(&pairs, false);
    if only_favorites {
        return Ok(Json(ApiDataTotalMap::ok(BTreeMap::new(), 0)));
    }
    filters.only_places = !filters.only_highlighted;
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;

    let realms = crate::handlers::places::with_realms_detail(&pairs);
    let mut map: BTreeMap<String, MapEntry> = BTreeMap::new();
    for place in &mut data {
        place.apply_realms_detail(realms);
        map.insert(place.base_position.clone(), MapEntry::from(&*place));
    }
    Ok(Json(ApiDataTotalMap::ok(map, total)))
}

#[utoipa::path(
    get,
    path = "/map/places",
    tag = "map",
    responses(
        (status = 200, body = ApiDataTotal<PlaceRow>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_all_places_list(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<PlaceRow>>, ApiError> {
    let (filters, only_favorites) = list_filters(&pairs, false);
    if only_favorites {
        return Ok(Json(ApiDataTotal::ok(vec![], 0)));
    }
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    let realms = crate::handlers::places::with_realms_detail(&pairs);
    for place in &mut data {
        place.apply_realms_detail(realms);
    }
    Ok(Json(ApiDataTotal::ok(data, total)))
}
