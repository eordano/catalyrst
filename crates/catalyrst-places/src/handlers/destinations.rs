use axum::extract::{Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::errors::ApiError;
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, PlaceRow};
use crate::AppState;

/// Aggregate flags parsed off the `/destinations` query string. Upstream gates
/// the comms/events enrichment behind these (`destinationsWithAggregates`).
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
        ..Default::default()
    };
    let flags = DestinationFlags {
        with_realms_detail: truthy("with_realms_detail"),
        with_connected_users: truthy("with_connected_users"),
        with_live_events: truthy("with_live_events"),
    };
    (f, only_favorites, flags)
}

fn to_destination(place: &PlaceRow) -> Value {
    let mut v = serde_json::to_value(place).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.remove("world_id");
    }
    v
}

/// Port of `destinationsWithAggregates` (entities/Destination/utils.ts): when
/// `with_connected_users` is set, attach `connected_addresses` (world_name key
/// for worlds, base_position for places) and bump `user_count` to the connected
/// count when it exceeds the stored count (`finalUserCount`); when
/// `with_live_events` is set, attach `live` (default false); when
/// `with_realms_detail` is set, attach `realms_detail` for places.
async fn enrich(state: &AppState, data: &mut [PlaceRow], flags: &DestinationFlags) {
    // Fetch connected users for every destination (worlds via world_name, places
    // via base_position), mirroring fetchConnectedUsersForDestinations.
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
            // finalUserCount: prefer the live connected count when larger.
            let connected_len = addresses.len() as i32;
            let base_count = d.user_count.unwrap_or(0);
            if connected_len > base_count {
                d.user_count = Some(connected_len);
            }
            d.connected_addresses = Some(addresses);
        }
    }

    // Live-event status: land places key on the place UUID, worlds on world_name.
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

    // realms_detail is experimental and only attached for non-world places. We
    // have no hot-scenes realm feed wired in, so it resolves to an empty list,
    // exactly as upstream does when a place has no matching hot scene.
    if flags.with_realms_detail {
        for d in data.iter_mut() {
            if !d.world {
                d.realms_detail = Some(Vec::new());
            }
        }
    }
}

pub async fn get_destinations_list(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<Value>, ApiError> {
    let (mut filters, only_favorites, flags) = parse_filters(&pairs);
    if only_favorites {
        return Ok(Json(json!({ "ok": true, "data": [], "total": 0 })));
    }
    if let Some(owner) = pairs.iter().find(|(k, _)| k == "owner").map(|(_, v)| v) {
        filters.operated_positions = state.places.operated_positions(owner).await?;
    }
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    enrich(&state, &mut data, &flags).await;
    let out: Vec<Value> = data.iter().map(to_destination).collect();
    Ok(Json(json!({ "ok": true, "data": out, "total": total })))
}

pub async fn post_destinations_list_by_id(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
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
        return Ok(Json(json!({ "ok": true, "data": [], "total": 0 })));
    }
    filters.ids = ids;
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    enrich(&state, &mut data, &flags).await;
    let out: Vec<Value> = data.iter().map(to_destination).collect();
    Ok(Json(json!({ "ok": true, "data": out, "total": total })))
}
