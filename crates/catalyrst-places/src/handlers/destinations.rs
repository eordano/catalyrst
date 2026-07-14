use std::collections::HashMap;

use axum::extract::{OriginalUri, Query, State};
use axum::http::{HeaderMap, Method};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::clients::{LiveUserCounts, NextEvent};
use crate::entity_id::EntityType;
use crate::http::errors::ApiError;
use crate::http::response::ApiDataTotal;
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, PlaceRow};
use crate::AppState;

pub const MAX_BATCH_ITEMS: usize = 1000;
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct DestinationNextEvent {
    pub id: String,
    pub name: String,
    pub next_start_at: String,
}

impl From<NextEvent> for DestinationNextEvent {
    fn from(e: NextEvent) -> Self {
        Self {
            id: e.id,
            name: e.name,
            next_start_at: e.next_start_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct Destination {
    pub id: String,
    pub kind: EntityType,
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
    pub live_event: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    #[cfg_attr(feature = "ts", ts(optional, type = "string | null"))]
    pub live_event_name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<DestinationNextEvent>)]
    #[cfg_attr(
        feature = "ts",
        ts(
            optional,
            type = "{ id: string, name: string, next_start_at: string } | null"
        )
    )]
    pub next_event: Option<Option<DestinationNextEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub connected_addresses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "Array<Record<string, unknown>>"))]
    pub realms_detail: Option<Vec<serde_json::Value>>,
}

impl From<PlaceRow> for Destination {
    fn from(p: PlaceRow) -> Self {
        let kind = if p.world {
            EntityType::World
        } else {
            EntityType::Place
        };
        Self {
            id: p.id,
            kind,
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
            live_event: p.live,
            live_event_name: p.live_event_name,
            next_event: None,
            connected_addresses: p.connected_addresses,
            realms_detail: p.realms_detail,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DestinationFlags {
    pub with_realms_detail: bool,
    pub with_connected_users: bool,
    pub with_live_events: bool,
    pub with_next_event: bool,
}

fn present(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|v| !v.is_empty())
}

// A caller naming what it wants back (a search, ids, its own or a creator's
// destinations, the curated shelf) is looking things up, not discovering:
// hiding a thumbnail-less row from that answer would read as the destination
// having vanished, so the content gate steps aside for both entity types.
fn is_lookup_query(f: &PlaceListFilters) -> bool {
    present(&f.search)
        || !f.ids.is_empty()
        || f.owner_filtered
        || present(&f.creator_address)
        || f.only_highlighted
}

// `owner` narrows the feed to that wallet's parcels, so it must set
// owner_filtered like /places does: without it an unknown owner is handed
// the whole feed, and this exemption would hand it ungated.
fn owner_param(pairs: &[(String, String)]) -> Option<&str> {
    pairs
        .iter()
        .find(|(k, _)| k == "owner")
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

// `positions` (or the legacy `pointer`) names parcels, so it exempts places
// only; `world_names` (or the legacy `names`) names worlds, so it exempts
// worlds only. Only a value that names something counts: `pointer=` matches
// no parcel, so it must not open the gate either.
fn require_feed_content(f: &mut PlaceListFilters) {
    let lookup = is_lookup_query(f);
    let names_parcels = f.positions.iter().any(|p| !p.is_empty());
    let names_worlds = f.names.iter().any(|n| !n.is_empty());
    f.require_content_places = !lookup && !names_parcels;
    f.require_content_worlds = !lookup && !names_worlds;
}

fn get<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn truthy(pairs: &[(String, String)], key: &str) -> bool {
    matches!(get(pairs, key), Some("true") | Some("1"))
}

// Upstream's multiParam: repeatable and comma-separated, trimmed, empties
// dropped, and capped so a GET cannot build a giant ANY(...) scan.
fn multi(pairs: &[(String, String)], key: &str) -> Result<Vec<String>, ApiError> {
    let values: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == key)
        .flat_map(|(_, v)| v.split(','))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect();
    if values.len() > MAX_BATCH_ITEMS {
        return Err(ApiError::bad_request(format!(
            "Too many {key} (max {MAX_BATCH_ITEMS})"
        )));
    }
    Ok(values)
}

// Parcels are "x,y" tokens, so they are never comma-split: `positions` is the
// unified name and `pointer` the legacy alias, both read whole. Upstream
// reads the alias through its comma-splitting multiParam, which shreds every
// parcel into two halves (an upstream bug), so this is a deliberate
// deviation: keep reading `pointer` whole.
fn positions_param(pairs: &[(String, String)], key: &str) -> Result<Vec<String>, ApiError> {
    let values: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
        .collect();
    if values.len() > MAX_BATCH_ITEMS {
        return Err(ApiError::bad_request(format!(
            "Too many {key} (max {MAX_BATCH_ITEMS})"
        )));
    }
    Ok(values)
}

// A non-numeric or negative limit/offset falls back to the default instead of
// reaching LIMIT/OFFSET.
fn int_param(pairs: &[(String, String)], key: &str) -> Option<i64> {
    get(pairs, key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|n| *n >= 0)
}

pub fn parse_with_options(pairs: &[(String, String)]) -> Result<DestinationFlags, ApiError> {
    let with = multi(pairs, "with")?;
    let has = |name: &str| with.iter().any(|w| w == name);
    Ok(DestinationFlags {
        with_live_events: truthy(pairs, "with_live_events") || has("live_events"),
        with_connected_users: truthy(pairs, "with_connected_users") || has("connected_users"),
        with_next_event: has("next_event"),
        with_realms_detail: truthy(pairs, "with_realms_detail") || has("realms_detail"),
    })
}

// The place and world branches of upstream's UNION are each kept by `kinds`
// and each keep the rows their own selector matches: a parcel selector alone
// excludes worlds, a world-name selector alone excludes places, and both
// together keep both. Both flags set at once means no branch survived.
fn select_branches(f: &mut PlaceListFilters, kinds: &[String]) {
    let wants_place = kinds.is_empty() || kinds.iter().any(|k| k == "place");
    let wants_world = kinds.is_empty() || kinds.iter().any(|k| k == "world");
    let excludes_worlds = !f.positions.is_empty() && f.names.is_empty();
    let excludes_places = !f.names.is_empty() && f.positions.is_empty();
    let place_branch = wants_place && !excludes_places;
    let world_branch = wants_world && !excludes_worlds;
    f.only_places = !world_branch;
    f.only_worlds = !place_branch;
}

fn no_branch(f: &PlaceListFilters) -> bool {
    f.only_places && f.only_worlds
}

pub(crate) fn parse_filters(
    pairs: &[(String, String)],
) -> Result<(PlaceListFilters, bool, DestinationFlags), ApiError> {
    let only_favorites = truthy(pairs, "only_favorites");
    let limit = int_param(pairs, "limit")
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT);
    let offset = int_param(pairs, "offset").unwrap_or(0);
    let mut kinds: Vec<String> = multi(pairs, "kinds")?
        .into_iter()
        .filter(|k| k == "place" || k == "world")
        .collect();
    if truthy(pairs, "only_worlds") {
        kinds = vec!["world".to_string()];
    } else if truthy(pairs, "only_places") {
        kinds = vec!["place".to_string()];
    }
    let mut positions = positions_param(pairs, "positions")?;
    if positions.is_empty() {
        positions = positions_param(pairs, "pointer")?;
    }
    let mut names = multi(pairs, "world_names")?;
    if names.is_empty() {
        names = multi(pairs, "names")?;
    }
    let sdk = if truthy(pairs, "only_sdk7") {
        Some("7".to_string())
    } else {
        get(pairs, "sdk").map(str::to_string)
    };
    let mut f = PlaceListFilters {
        limit,
        offset,
        positions,
        categories: multi(pairs, "categories")?,
        names,
        ids: multi(pairs, "ids")?
            .into_iter()
            .map(|id| id.to_lowercase())
            .collect(),
        only_highlighted: truthy(pairs, "only_highlighted"),
        search: get(pairs, "search").map(str::to_string),
        creator_address: get(pairs, "creator_address").map(|s| s.to_lowercase()),
        sdk,
        order_by: PlaceOrderBy::parse(get(pairs, "order_by")),
        order_desc: !matches!(get(pairs, "order"), Some("asc")),
        destinations_mode: true,
        ..Default::default()
    };
    select_branches(&mut f, &kinds);
    Ok((f, only_favorites, parse_with_options(pairs)?))
}

async fn inject_live_user_counts(state: &AppState, filters: &mut PlaceListFilters) {
    if !matches!(filters.order_by, PlaceOrderBy::MostActive) {
        return;
    }
    let counts = state.presence.live_user_counts().await;
    filters.place_user_counts = counts.places;
    filters.world_user_counts = counts.worlds;
}

fn event_key(d: &PlaceRow) -> String {
    if d.world {
        d.world_name.clone().unwrap_or_else(|| d.id.clone())
    } else {
        d.id.clone()
    }
}

// Upstream decorates every destination with its live occupancy whether or
// not the caller asked, and a destination nobody is in is empty, not
// unknown. Places are keyed by their parcel and worlds by their name.
fn fill_user_counts(rows: &mut [PlaceRow], counts: &LiveUserCounts) {
    let places: HashMap<&str, i32> = counts
        .places
        .iter()
        .map(|(pointer, n)| (pointer.as_str(), *n))
        .collect();
    let worlds: HashMap<String, i32> = counts
        .worlds
        .iter()
        .map(|(name, n)| (name.to_lowercase(), *n))
        .collect();
    for row in rows.iter_mut() {
        let count = if row.world {
            let name = row.world_name.as_deref().unwrap_or(&row.id);
            worlds.get(&name.to_lowercase())
        } else {
            places.get(row.base_position.as_str())
        };
        row.user_count = Some(count.copied().unwrap_or(0));
    }
}

pub(crate) async fn enrich(state: &AppState, data: &mut [PlaceRow], flags: &DestinationFlags) {
    if data.is_empty() {
        return;
    }
    let counts = state.presence.live_user_counts().await;
    fill_user_counts(data, &counts);

    if flags.with_connected_users {
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

    if flags.with_live_events {
        let ids: Vec<String> = data.iter().map(event_key).collect();
        let live_map = state.events.check_live_events(&ids).await;
        for d in data.iter_mut() {
            let event_name = live_map.get(&event_key(d)).cloned().unwrap_or(None);
            d.live = Some(event_name.is_some());
            d.live_event_name = Some(event_name);
        }
    }

    if flags.with_realms_detail {
        for d in data.iter_mut() {
            d.apply_realms_detail(true);
        }
    }
}

fn destination_event_key(d: &Destination) -> String {
    if d.world {
        d.world_name.clone().unwrap_or_else(|| d.id.clone())
    } else {
        d.id.clone()
    }
}

pub(crate) async fn decorate_next_event(
    state: &AppState,
    data: &mut [Destination],
    flags: &DestinationFlags,
) {
    if !flags.with_next_event || data.is_empty() {
        return;
    }
    let ids: Vec<String> = data.iter().map(destination_event_key).collect();
    let next = state.events.next_events(&ids).await;
    for d in data.iter_mut() {
        d.next_event = Some(
            next.get(&destination_event_key(d))
                .cloned()
                .map(DestinationNextEvent::from),
        );
    }
}

fn empty() -> Json<ApiDataTotal<Destination>> {
    Json(ApiDataTotal::ok(vec![], 0))
}

// Favorites are per-user: an anonymous only_favorites query is empty, and a
// signed one narrows the id set to what that wallet favorited.
async fn narrow_to_favorites(
    state: &AppState,
    user: Option<&str>,
    filters: &mut PlaceListFilters,
) -> Result<bool, ApiError> {
    let Some(addr) = user else {
        return Ok(false);
    };
    let Some(favorites) = state.places.favorite_entity_ids(addr).await? else {
        return Ok(false);
    };
    if favorites.is_empty() {
        return Ok(false);
    }
    if filters.ids.is_empty() {
        filters.ids = favorites;
    } else {
        filters.ids.retain(|id| favorites.contains(id));
    }
    Ok(!filters.ids.is_empty())
}

async fn run_list(
    state: &AppState,
    user: Option<&str>,
    mut filters: PlaceListFilters,
    flags: &DestinationFlags,
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    if no_branch(&filters) {
        return Ok(empty());
    }
    require_feed_content(&mut filters);
    inject_live_user_counts(state, &mut filters).await;
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    state.places.apply_user_interactions(user, &mut data).await;
    enrich(state, &mut data, flags).await;
    let mut out: Vec<Destination> = data.into_iter().map(Destination::from).collect();
    decorate_next_event(state, &mut out, flags).await;
    Ok(Json(ApiDataTotal::ok(out, total)))
}

// The caller's identity is optional but never taken on trust: `signed_path`
// is the route the caller signed, so a forged auth chain reads as anonymous.
pub(crate) async fn list_destinations(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    signed_path: &str,
    pairs: &[(String, String)],
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    let (mut filters, only_favorites, flags) = parse_filters(pairs)?;
    let user = crate::auth::auth_address_optional(headers, method, signed_path).await;
    if only_favorites && !narrow_to_favorites(state, user.as_deref(), &mut filters).await? {
        return Ok(empty());
    }
    if let Some(owner) = owner_param(pairs) {
        filters.owner_filtered = true;
        filters.operated_positions = state.places.operated_positions(owner).await?;
    }
    run_list(state, user.as_deref(), filters, &flags).await
}

#[utoipa::path(
    get,
    path = "/destinations",
    tag = "destinations",
    params(("limit" = Option<i64>, Query),
        ("offset" = Option<i64>, Query),
        ("ids" = Option<Vec<String>>, Query),
        ("positions" = Option<Vec<String>>, Query),
        ("pointer" = Option<Vec<String>>, Query),
        ("world_names" = Option<Vec<String>>, Query),
        ("names" = Option<Vec<String>>, Query),
        ("kinds" = Option<Vec<String>>, Query),
        ("only_places" = Option<String>, Query),
        ("only_worlds" = Option<String>, Query),
        ("categories" = Option<Vec<String>>, Query),
        ("only_highlighted" = Option<String>, Query),
        ("only_favorites" = Option<String>, Query),
        ("search" = Option<String>, Query),
        ("owner" = Option<String>, Query),
        ("creator_address" = Option<String>, Query),
        ("sdk" = Option<String>, Query),
        ("only_sdk7" = Option<String>, Query),
        ("order_by" = Option<String>, Query),
        ("order" = Option<String>, Query),
        ("with" = Option<Vec<String>>, Query),
        ("with_live_events" = Option<String>, Query),
        ("with_connected_users" = Option<String>, Query),
        ("with_realms_detail" = Option<String>, Query)),
    responses(
        (status = 200, body = ApiDataTotal<Destination>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_destinations_list(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    list_destinations(&state, &headers, method.as_str(), uri.path(), &pairs).await
}

// A body array of strings, capped like the query selectors; None when absent.
fn bounded_list(value: Option<&Value>) -> Result<Option<Vec<String>>, ApiError> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Ok(None);
    };
    if items.len() > MAX_BATCH_ITEMS {
        return Err(ApiError::bad_request(format!(
            "Too many items (max {MAX_BATCH_ITEMS})"
        )));
    }
    Ok(Some(
        items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    ))
}

struct BatchSelectors {
    ids: Vec<String>,
    positions: Vec<String>,
    world_names: Vec<String>,
}

// Legacy callers send a bare array of ids; newer ones an object of selectors.
// A selector that was provided but survived validation empty is a by-id
// lookup for nothing, never a fall-through to the unfiltered feed.
fn parse_batch_body(body: &Value) -> Result<Option<BatchSelectors>, ApiError> {
    let object;
    let body = if body.is_array() {
        object = serde_json::json!({ "ids": body });
        &object
    } else {
        body
    };
    let ids = bounded_list(body.get("ids"))?;
    let positions = bounded_list(body.get("positions"))?;
    let world_names = bounded_list(body.get("world_names"))?;
    let provided = ids.is_some() || positions.is_some() || world_names.is_some();
    let ids = ids.unwrap_or_default();
    let positions = positions.unwrap_or_default();
    let world_names = world_names.unwrap_or_default();
    if provided && ids.is_empty() && positions.is_empty() && world_names.is_empty() {
        return Ok(None);
    }
    Ok(Some(BatchSelectors {
        ids: ids.into_iter().map(|id| id.to_lowercase()).collect(),
        positions,
        world_names,
    }))
}

#[utoipa::path(
    post,
    path = "/destinations",
    tag = "destinations",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = ApiDataTotal<Destination>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn post_destinations_list_by_id(
    State(state): State<AppState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiDataTotal<Destination>>, ApiError> {
    let body = body
        .map(|Json(v)| v)
        .unwrap_or_else(|| Value::Object(Default::default()));
    let Some(selectors) = parse_batch_body(&body)? else {
        return Ok(empty());
    };
    let flags = parse_with_options(&pairs)?;
    let user = crate::auth::auth_address_optional(&headers, method.as_str(), uri.path()).await;
    let mut filters = PlaceListFilters {
        limit: MAX_LIMIT,
        ids: selectors.ids,
        positions: selectors.positions,
        names: selectors.world_names,
        destinations_mode: true,
        ..Default::default()
    };
    select_branches(&mut filters, &[]);
    run_list(&state, user.as_deref(), filters, &flags).await
}

#[cfg(test)]
mod feed_gate_tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn gated(list: &[(&str, &str)]) -> (bool, bool) {
        let pairs = pairs(list);
        let (mut f, _, _) = parse_filters(&pairs).unwrap();
        f.owner_filtered = owner_param(&pairs).is_some();
        require_feed_content(&mut f);
        (f.require_content_places, f.require_content_worlds)
    }

    #[test]
    fn the_generic_feed_gates_both_branches() {
        assert_eq!(gated(&[]), (true, true));
        assert_eq!(gated(&[("only_places", "true")]), (true, true));
        assert_eq!(gated(&[("order_by", "most_active")]), (true, true));
    }

    #[test]
    fn filters_that_do_not_bypass_the_check() {
        assert_eq!(gated(&[("categories", "art")]), (true, true));
        assert_eq!(gated(&[("sdk", "7")]), (true, true));
    }

    #[test]
    fn lookup_filters_exempt_both_branches() {
        assert_eq!(gated(&[("search", "junk")]), (false, false));
        assert_eq!(gated(&[("creator_address", "0xABC")]), (false, false));
        assert_eq!(gated(&[("only_highlighted", "true")]), (false, false));
        assert_eq!(gated(&[("owner", "0xABC")]), (false, false));
        assert_eq!(gated(&[("ids", "uuid-1")]), (false, false));
    }

    #[test]
    fn empty_lookup_values_do_not_count_as_lookups() {
        assert_eq!(gated(&[("search", "")]), (true, true));
        assert_eq!(gated(&[("creator_address", "")]), (true, true));
        assert_eq!(gated(&[("pointer", "")]), (true, true));
        assert_eq!(gated(&[("names", "")]), (true, true));
        assert_eq!(gated(&[("pointer", ""), ("names", "")]), (true, true));
        assert_eq!(gated(&[("pointer", ""), ("pointer", "1,1")]), (false, true));
    }

    #[test]
    fn pointer_exempts_places_only_and_names_exempts_worlds_only() {
        assert_eq!(gated(&[("pointer", "1,1")]), (false, true));
        assert_eq!(gated(&[("positions", "1,1")]), (false, true));
        assert_eq!(gated(&[("names", "junk")]), (true, false));
        assert_eq!(gated(&[("world_names", "junk")]), (true, false));
        assert_eq!(
            gated(&[("pointer", "1,1"), ("names", "junk")]),
            (false, false)
        );
    }

    #[test]
    fn an_owner_value_narrows_the_feed_and_only_then_exempts_it() {
        let list = pairs(&[("owner", "0x000000000000000000000000000000000000dead")]);
        assert_eq!(
            owner_param(&list),
            Some("0x000000000000000000000000000000000000dead")
        );
        let (mut f, _, _) = parse_filters(&list).unwrap();
        f.owner_filtered = owner_param(&list).is_some();
        require_feed_content(&mut f);
        assert!(f.owner_filtered);
        assert_eq!(
            (f.require_content_places, f.require_content_worlds),
            (false, false)
        );

        let empty = pairs(&[("owner", "")]);
        assert_eq!(owner_param(&empty), None);
        let (mut f, _, _) = parse_filters(&empty).unwrap();
        f.owner_filtered = owner_param(&empty).is_some();
        require_feed_content(&mut f);
        assert!(!f.owner_filtered);
        assert_eq!(
            (f.require_content_places, f.require_content_worlds),
            (true, true)
        );
    }

    #[test]
    fn ids_lookup_exempts_both_branches() {
        let (mut f, _, _) = parse_filters(&pairs(&[])).unwrap();
        f.ids = vec!["uuid-1".to_string()];
        require_feed_content(&mut f);
        assert!(!f.require_content_places);
        assert!(!f.require_content_worlds);
    }
}

#[cfg(test)]
mod param_parity_tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn filters(list: &[(&str, &str)]) -> PlaceListFilters {
        parse_filters(&pairs(list)).unwrap().0
    }

    #[test]
    fn pagination_defaults_and_bounds_follow_upstream() {
        let f = filters(&[]);
        assert_eq!((f.limit, f.offset), (50, 0));
        let f = filters(&[("limit", "500"), ("offset", "7")]);
        assert_eq!((f.limit, f.offset), (100, 7));
        let f = filters(&[("limit", "-3"), ("offset", "abc")]);
        assert_eq!((f.limit, f.offset), (50, 0));
        let f = filters(&[("limit", "0")]);
        assert_eq!(f.limit, 0);
    }

    #[test]
    fn comma_separated_and_repeated_multi_params_merge() {
        let f = filters(&[("categories", "art, game"), ("categories", "music")]);
        assert_eq!(f.categories, vec!["art", "game", "music"]);
        let f = filters(&[("ids", "AAA,bbb")]);
        assert_eq!(f.ids, vec!["aaa", "bbb"]);
    }

    #[test]
    fn parcels_are_never_comma_split() {
        let f = filters(&[("positions", "10,20"), ("positions", "-3,4")]);
        assert_eq!(f.positions, vec!["10,20", "-3,4"]);
        let f = filters(&[("pointer", "10,20")]);
        assert_eq!(f.positions, vec!["10,20"]);
        let f = filters(&[("positions", "1,1"), ("pointer", "2,2")]);
        assert_eq!(f.positions, vec!["1,1"]);
    }

    #[test]
    fn world_names_takes_precedence_over_the_legacy_names_alias() {
        let f = filters(&[("names", "a.dcl.eth")]);
        assert_eq!(f.names, vec!["a.dcl.eth"]);
        let f = filters(&[("world_names", "b.dcl.eth"), ("names", "a.dcl.eth")]);
        assert_eq!(f.names, vec!["b.dcl.eth"]);
    }

    #[test]
    fn kinds_and_legacy_only_flags_narrow_the_branches() {
        let f = filters(&[("kinds", "world")]);
        assert!(f.only_worlds && !f.only_places);
        let f = filters(&[("kinds", "place")]);
        assert!(f.only_places && !f.only_worlds);
        let f = filters(&[("kinds", "place,world")]);
        assert!(!f.only_places && !f.only_worlds);
        let f = filters(&[("kinds", "junk")]);
        assert!(!f.only_places && !f.only_worlds);
        let f = filters(&[("kinds", "place"), ("only_worlds", "true")]);
        assert!(f.only_worlds && !f.only_places);
        let f = filters(&[("only_places", "true")]);
        assert!(f.only_places && !f.only_worlds);
    }

    #[test]
    fn a_lone_selector_excludes_the_other_branch() {
        let f = filters(&[("positions", "1,1")]);
        assert!(f.only_places && !f.only_worlds);
        let f = filters(&[("world_names", "a.dcl.eth")]);
        assert!(f.only_worlds && !f.only_places);
        let f = filters(&[("positions", "1,1"), ("world_names", "a.dcl.eth")]);
        assert!(!f.only_places && !f.only_worlds);
        let f = filters(&[("positions", "1,1"), ("kinds", "world")]);
        assert!(
            no_branch(&f),
            "a parcel selector leaves the world branch nothing"
        );
        let f = filters(&[("positions", "1,1"), ("kinds", "place,world")]);
        assert!(f.only_places && !f.only_worlds);
        let f = filters(&[("world_names", "a.dcl.eth"), ("kinds", "world")]);
        assert!(f.only_worlds && !f.only_places);
    }

    #[test]
    fn only_sdk7_is_shorthand_for_sdk_7() {
        assert_eq!(filters(&[("only_sdk7", "true")]).sdk.as_deref(), Some("7"));
        assert_eq!(filters(&[("sdk", "6")]).sdk.as_deref(), Some("6"));
        assert_eq!(filters(&[("only_sdk7", "false")]).sdk, None);
    }

    #[test]
    fn with_list_and_legacy_boolean_aliases_both_decorate() {
        let flags = parse_with_options(&pairs(&[(
            "with",
            "live_events,connected_users,next_event,realms_detail",
        )]))
        .unwrap();
        assert!(flags.with_live_events);
        assert!(flags.with_connected_users);
        assert!(flags.with_next_event);
        assert!(flags.with_realms_detail);
        let flags = parse_with_options(&pairs(&[
            ("with_live_events", "true"),
            ("with_realms_detail", "true"),
        ]))
        .unwrap();
        assert!(flags.with_live_events);
        assert!(!flags.with_connected_users);
        assert!(!flags.with_next_event);
        assert!(flags.with_realms_detail);
    }

    #[test]
    fn oversized_selector_lists_are_rejected() {
        let ids = (0..=MAX_BATCH_ITEMS)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_filters(&pairs(&[("ids", ids.as_str())])).is_err());
    }

    #[test]
    fn batch_body_accepts_the_bare_array_and_the_selector_object() {
        let bare = parse_batch_body(&serde_json::json!(["A", 7]))
            .unwrap()
            .expect("ids selected");
        assert_eq!(bare.ids, vec!["a"]);
        let object = parse_batch_body(&serde_json::json!({
            "positions": ["3,3"],
            "world_names": ["w.dcl.eth"]
        }))
        .unwrap()
        .expect("selectors present");
        assert!(object.ids.is_empty());
        assert_eq!(object.positions, vec!["3,3"]);
        assert_eq!(object.world_names, vec!["w.dcl.eth"]);
    }

    #[test]
    fn a_provided_but_empty_selector_answers_nothing_not_everything() {
        assert!(parse_batch_body(&serde_json::json!({ "ids": [42] }))
            .unwrap()
            .is_none());
        assert!(parse_batch_body(&serde_json::json!([])).unwrap().is_none());
        let unfiltered = parse_batch_body(&serde_json::json!({}))
            .unwrap()
            .expect("no selector means the open list");
        assert!(unfiltered.ids.is_empty() && unfiltered.positions.is_empty());
        let too_many: Vec<String> = (0..=MAX_BATCH_ITEMS).map(|i| i.to_string()).collect();
        assert!(parse_batch_body(&serde_json::json!({ "ids": too_many })).is_err());
    }
}

#[cfg(test)]
mod live_event_name_tests {
    use super::*;

    fn destination() -> Destination {
        Destination {
            id: "uuid-1".to_string(),
            kind: EntityType::Place,
            title: Some("Amber Hollow".to_string()),
            description: None,
            image: None,
            owner: None,
            positions: vec!["1,2".to_string()],
            base_position: "1,2".to_string(),
            contact_name: None,
            contact_email: None,
            content_rating: None,
            disabled: false,
            disabled_at: None,
            created_at: None,
            updated_at: None,
            favorites: 0,
            likes: 0,
            dislikes: 0,
            categories: vec![],
            highlighted: false,
            highlighted_image: None,
            ranking: None,
            sdk: None,
            creator_address: None,
            deployed_at: None,
            world: false,
            world_name: None,
            is_private: false,
            user_favorite: false,
            user_like: false,
            user_dislike: false,
            user_count: None,
            user_visits: 0,
            like_rate: None,
            like_score: None,
            live: None,
            live_event: None,
            live_event_name: None,
            next_event: None,
            connected_addresses: None,
            realms_detail: None,
        }
    }

    #[test]
    fn omitted_when_live_events_were_not_requested() {
        let v = serde_json::to_value(destination()).unwrap();
        assert!(v.get("live").is_none());
        assert!(v.get("live_event").is_none());
        assert!(v.get("live_event_name").is_none());
        assert!(v.get("next_event").is_none());
    }

    #[test]
    fn named_alongside_the_live_flag() {
        let mut d = destination();
        d.live = Some(true);
        d.live_event = Some(true);
        d.live_event_name = Some(Some("Movie Night".to_string()));
        let v = serde_json::to_value(d).unwrap();
        assert_eq!(v["live"], serde_json::json!(true));
        assert_eq!(v["live_event"], serde_json::json!(true));
        assert_eq!(v["live_event_name"], serde_json::json!("Movie Night"));
    }

    #[test]
    fn null_on_a_destination_without_a_live_event() {
        let mut d = destination();
        d.live = Some(false);
        d.live_event = Some(false);
        d.live_event_name = Some(None);
        let v = serde_json::to_value(d).unwrap();
        assert_eq!(v["live"], serde_json::json!(false));
        assert_eq!(v["live_event"], serde_json::json!(false));
        assert!(v.get("live_event_name").is_some());
        assert_eq!(v["live_event_name"], serde_json::Value::Null);
    }

    #[test]
    fn next_event_is_null_when_requested_and_absent() {
        let mut d = destination();
        d.next_event = Some(None);
        let v = serde_json::to_value(d).unwrap();
        assert_eq!(v["next_event"], serde_json::Value::Null);
        let mut d = destination();
        d.next_event = Some(Some(DestinationNextEvent {
            id: "ev-1".to_string(),
            name: "Upcoming".to_string(),
            next_start_at: "2030-01-01T00:00:00Z".to_string(),
        }));
        let v = serde_json::to_value(d).unwrap();
        assert_eq!(v["next_event"]["name"], serde_json::json!("Upcoming"));
    }

    #[test]
    fn kind_and_live_event_mirror_the_row() {
        let mut row = destination_row();
        row.world = true;
        row.live = Some(true);
        let d = Destination::from(row);
        assert_eq!(d.kind, EntityType::World);
        assert_eq!(d.live_event, Some(true));
        assert_eq!(d.live, Some(true));
        assert_eq!(
            serde_json::to_value(d).unwrap()["kind"],
            serde_json::json!("world")
        );
    }

    #[test]
    fn every_row_gets_its_live_occupancy_keyed_by_parcel_or_world_name() {
        let mut place = destination_row();
        place.base_position = "10,20".to_string();
        let mut world = destination_row();
        world.id = "world-row".to_string();
        world.world = true;
        world.world_name = Some("My-World.dcl.eth".to_string());
        let mut idle = destination_row();
        idle.id = "idle".to_string();
        idle.user_count = None;
        let counts = LiveUserCounts {
            places: vec![("10,20".to_string(), 4)],
            worlds: vec![("my-world.dcl.eth".to_string(), 2)],
        };
        let mut rows = vec![place, world, idle];
        fill_user_counts(&mut rows, &counts);
        assert_eq!(rows[0].user_count, Some(4));
        assert_eq!(rows[1].user_count, Some(2));
        assert_eq!(
            rows[2].user_count,
            Some(0),
            "a destination nobody is in is empty, not unknown"
        );
    }

    fn destination_row() -> crate::ports::places::PlaceRow {
        crate::ports::places::PlaceRow {
            id: "uuid-1".to_string(),
            title: None,
            description: None,
            image: None,
            owner: None,
            positions: vec![],
            base_position: "0,0".to_string(),
            contact_name: None,
            contact_email: None,
            content_rating: None,
            disabled: false,
            disabled_at: None,
            disabled_reason: None,
            created_at: None,
            updated_at: None,
            favorites: 0,
            likes: 0,
            dislikes: 0,
            categories: vec![],
            highlighted: false,
            highlighted_image: None,
            ranking: None,
            exclude_from_ranking: false,
            sdk: None,
            creator_address: None,
            world_id: None,
            deployment_id: None,
            deployed_at: None,
            world: false,
            world_name: None,
            is_private: false,
            show_in_places: true,
            single_player: false,
            skybox_time: None,
            user_favorite: false,
            user_like: false,
            user_dislike: false,
            user_count: None,
            user_visits: 0,
            like_rate: None,
            like_score: None,
            live: None,
            live_event_name: None,
            connected_addresses: None,
            realms_detail: None,
        }
    }
}
