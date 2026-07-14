use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::http::errors::ApiError;
use crate::http::response::ApiDataTotal;
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, PlaceRow};
use crate::AppState;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct Destination {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub owner: Option<String>,
    pub positions: Vec<String>,
    pub base_position: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub content_rating: Option<String>,
    pub disabled: bool,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub disabled_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub created_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub updated_at: Option<DateTime<Utc>>,
    pub favorites: i32,
    pub likes: i32,
    pub dislikes: i32,
    pub categories: Vec<String>,
    pub highlighted: bool,
    pub highlighted_image: Option<String>,
    pub ranking: Option<f64>,
    pub sdk: Option<String>,
    pub creator_address: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub deployed_at: Option<DateTime<Utc>>,
    pub world: bool,
    pub world_name: Option<String>,
    pub is_private: bool,
    pub user_favorite: bool,
    pub user_like: bool,
    pub user_dislike: bool,
    pub user_count: Option<i32>,
    pub user_visits: i32,
    pub like_rate: Option<f64>,
    pub like_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub connected_addresses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "Array<Record<string, unknown>>"))]
    pub realms_detail: Option<Vec<serde_json::Value>>,
}

impl From<PlaceRow> for Destination {
    fn from(p: PlaceRow) -> Self {
        Self {
            id: p.id,
            title: p.title,
            description: p.description,
            image: p.image,
            owner: p.owner,
            positions: p.positions,
            base_position: p.base_position,
            contact_name: p.contact_name,
            contact_email: p.contact_email,
            content_rating: p.content_rating,
            disabled: p.disabled,
            disabled_at: p.disabled_at,
            created_at: p.created_at,
            updated_at: p.updated_at,
            favorites: p.favorites,
            likes: p.likes,
            dislikes: p.dislikes,
            categories: p.categories,
            highlighted: p.highlighted,
            highlighted_image: p.highlighted_image,
            ranking: p.ranking,
            sdk: p.sdk,
            creator_address: p.creator_address,
            deployed_at: p.deployed_at,
            world: p.world,
            world_name: p.world_name,
            is_private: p.is_private,
            user_favorite: p.user_favorite,
            user_like: p.user_like,
            user_dislike: p.user_dislike,
            user_count: p.user_count,
            user_visits: p.user_visits,
            like_rate: p.like_rate,
            like_score: p.like_score,
            live: p.live,
            connected_addresses: p.connected_addresses,
            realms_detail: p.realms_detail,
        }
    }
}

struct DestinationFlags {
    with_realms_detail: bool,
    with_connected_users: bool,
    with_live_events: bool,
}

fn parse_filters(pairs: &[(String, String)]) -> (PlaceListFilters, bool, DestinationFlags) {
    let get = |k: &str| pairs.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
    let get_all = |k: &str| {
        pairs
            .iter()
            .filter(|(p, _)| p == k)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    let truthy = |k: &str| {
        get(k)
            .map(|v| matches!(v.as_str(), "true" | "1"))
            .unwrap_or(false)
    };
    let only_favorites = truthy("only_favorites");
    let limit = get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 100);
    let offset = get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let only_worlds = truthy("only_worlds");
    let only_places = truthy("only_places");
    let f = PlaceListFilters {
        limit,
        offset,
        positions: get_all("pointer"),
        categories: get_all("categories"),
        names: get_all("names"),
        only_highlighted: truthy("only_highlighted"),
        search: get("search"),
        creator_address: get("creator_address").map(|s| s.to_lowercase()),
        sdk: get("sdk"),
        order_by: PlaceOrderBy::parse(get("order_by").as_deref()),
        order_desc: !matches!(get("order").as_deref(), Some("asc")),
        only_worlds,
        only_places,
        destinations_mode: true,
        ..Default::default()
    };
    let flags = DestinationFlags {
        with_realms_detail: truthy("with_realms_detail"),
        with_connected_users: truthy("with_connected_users"),
        with_live_events: truthy("with_live_events"),
    };
    (f, only_favorites, flags)
}

async fn inject_live_user_counts(state: &AppState, filters: &mut PlaceListFilters) {
    if !matches!(filters.order_by, PlaceOrderBy::MostActive) {
        return;
    }
    let counts = state.presence.live_user_counts().await;
    filters.place_user_counts = counts.places;
    filters.world_user_counts = counts.worlds;
}

async fn enrich(state: &AppState, data: &mut [PlaceRow], flags: &DestinationFlags) {
    if flags.with_connected_users && !data.is_empty() {
        for d in data.iter_mut() {
            let addresses = if d.world {
                match d.world_name.as_deref() {
                    Some(name) => state.comms_gatekeeper.get_world_participants(name).await,
                    None => Vec::new(),
                }
            } else {
                state
                    .comms_gatekeeper
                    .get_scene_participants(&d.base_position)
                    .await
            };

            let connected_len = addresses.len() as i32;
            let base_count = d.user_count.unwrap_or(0);
            if connected_len > base_count {
                d.user_count = Some(connected_len);
            }
            d.connected_addresses = Some(addresses);
        }
    }

    if flags.with_live_events && !data.is_empty() {
        let ids: Vec<String> = data
            .iter()
            .map(|d| {
                if d.world {
                    d.world_name.clone().unwrap_or_else(|| d.id.clone())
                } else {
                    d.id.clone()
                }
            })
            .collect();
        let live_map = state.events.check_live_events(&ids).await;
        for d in data.iter_mut() {
            let key = if d.world {
                d.world_name.clone().unwrap_or_else(|| d.id.clone())
            } else {
                d.id.clone()
            };
            d.live = Some(*live_map.get(&key).unwrap_or(&false));
        }
    }

    if flags.with_realms_detail {
        for d in data.iter_mut() {
            d.apply_realms_detail(true);
        }
    }
}

#[utoipa::path(
    get,
    path = "/destinations",
    tag = "destinations",
    params(("limit" = Option<i64>, Query), ("offset" = Option<i64>, Query)),
    responses(
        (status = 200, body = ApiDataTotal<Destination>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_destinations_list(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    let (mut filters, only_favorites, flags) = parse_filters(&pairs);
    if only_favorites {
        return Ok(Json(ApiDataTotal::ok(vec![], 0)));
    }
    if let Some(owner) = pairs.iter().find(|(k, _)| k == "owner").map(|(_, v)| v) {
        filters.operated_positions = state.places.operated_positions(owner).await?;
    }
    inject_live_user_counts(&state, &mut filters).await;
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    enrich(&state, &mut data, &flags).await;
    let out: Vec<Destination> = data.into_iter().map(Destination::from).collect();
    Ok(Json(ApiDataTotal::ok(out, total)))
}

#[utoipa::path(
    post,
    path = "/destinations",
    tag = "destinations",
    request_body = Vec<String>,
    responses(
        (status = 200, body = ApiDataTotal<Destination>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn post_destinations_list_by_id(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
    Json(body): Json<Value>,
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    let ids = body
        .as_array()
        .ok_or_else(|| {
            ApiError::bad_request("Invalid request body. Expected an array of destination IDs.")
        })?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    if ids.len() > 100 {
        return Err(ApiError::bad_request(
            "Cannot request more than 100 destinations at once",
        ));
    }
    let (mut filters, only_favorites, flags) = parse_filters(&pairs);
    if only_favorites {
        return Ok(Json(ApiDataTotal::ok(vec![], 0)));
    }
    filters.ids = ids;
    inject_live_user_counts(&state, &mut filters).await;
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    enrich(&state, &mut data, &flags).await;
    let out: Vec<Destination> = data.into_iter().map(Destination::from).collect();
    Ok(Json(ApiDataTotal::ok(out, total)))
}
