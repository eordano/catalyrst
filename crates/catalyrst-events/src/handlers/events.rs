use crate::http::response::{ApiError, ApiErrorBody, ApiOk};
use crate::ports::events::{EventListFilters, EventListType, SortOrder};
use crate::schemas::EventRecord;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventListWithTotal {
    pub events: Vec<EventRecord>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(untagged)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub enum EventListData {
    WithTotal(EventListWithTotal),
    Events(Vec<EventRecord>),
}

#[derive(Debug, Deserialize, Default)]
pub struct EventListQuery {
    pub limit: Option<String>,
    pub offset: Option<String>,
    pub list: Option<String>,
    pub order: Option<String>,
    pub highlighted: Option<String>,
    pub creator: Option<String>,
    pub world: Option<String>,
    pub world_names: Option<Vec<String>>,
    pub position: Option<String>,
    pub positions: Option<Vec<String>>,
    pub estate_id: Option<String>,
    pub community_id: Option<String>,
    pub places_ids: Option<Vec<String>>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
    pub only_attendee: Option<String>,
    pub schedule: Option<String>,
    pub owner: Option<String>,
    pub with_connected_users: Option<String>,
    pub approved: Option<String>,
    pub rejected: Option<String>,
    pub deleted: Option<String>,
}

impl EventListQuery {
    fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut q = EventListQuery::default();
        let (mut positions, mut world_names, mut places_ids) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in pairs {
            match k.as_str() {
                "limit" => q.limit = Some(v.clone()),
                "offset" => q.offset = Some(v.clone()),
                "list" => q.list = Some(v.clone()),
                "order" => q.order = Some(v.clone()),
                "highlighted" => q.highlighted = Some(v.clone()),
                "creator" => q.creator = Some(v.clone()),
                "world" => q.world = Some(v.clone()),
                "position" => q.position = Some(v.clone()),
                "estate_id" => q.estate_id = Some(v.clone()),
                "community_id" => q.community_id = Some(v.clone()),
                "from" => q.from = Some(v.clone()),
                "to" => q.to = Some(v.clone()),
                "search" => q.search = Some(v.clone()),
                "only_attendee" => q.only_attendee = Some(v.clone()),
                "schedule" => q.schedule = Some(v.clone()),
                "owner" => q.owner = Some(v.clone()),
                "with_connected_users" => q.with_connected_users = Some(v.clone()),
                "approved" => q.approved = Some(v.clone()),
                "rejected" => q.rejected = Some(v.clone()),
                "deleted" => q.deleted = Some(v.clone()),
                "positions" | "positions[]" => positions.push(v.clone()),
                "world_names" | "world_names[]" => world_names.push(v.clone()),
                "places_ids" | "places_ids[]" => places_ids.push(v.clone()),
                _ => {}
            }
        }
        if !positions.is_empty() {
            q.positions = Some(positions);
        }
        if !world_names.is_empty() {
            q.world_names = Some(world_names);
        }
        if !places_ids.is_empty() {
            q.places_ids = Some(places_ids);
        }
        q
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_position(s: &str) -> Option<(i32, i32)> {
    let mut it = s.splitn(2, ',');
    let x = it.next()?.parse::<i32>().ok()?;
    let y = it.next()?.parse::<i32>().ok()?;
    Some((x, y))
}

fn parse_filters(
    q: &EventListQuery,
    body_place_ids: Vec<String>,
    body_community_id: Option<String>,
    user: Option<String>,
    admin: bool,
) -> Result<Option<EventListFilters>, ApiError> {
    let limit = q
        .limit
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .map(|n| n.clamp(0, 500))
        .unwrap_or(500);
    if limit == 0 {
        return Ok(None);
    }
    let offset = q
        .offset
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .map(|n| n.max(0))
        .unwrap_or(0);
    let mut list = match q.list.as_deref() {
        Some("all") => EventListType::All,
        Some("live") => EventListType::Live,
        Some("upcoming") => EventListType::Upcoming,
        Some("relevance") => EventListType::Relevance,
        Some("highlight") => EventListType::Active,
        _ => EventListType::Active,
    };
    let order = match q.order.as_deref() {
        Some("desc") => SortOrder::Desc,
        Some("asc") => SortOrder::Asc,
        _ if q.search.is_some() => SortOrder::Desc,
        _ => SortOrder::Asc,
    };
    let mut highlighted = q.highlighted.as_deref().and_then(parse_bool);
    if matches!(q.list.as_deref(), Some("highlight")) {
        highlighted = Some(true);
        list = EventListType::Active;
    }

    let mut positions: Vec<(i32, i32)> = Vec::new();
    if let Some(p) = &q.position {
        match parse_position(p) {
            Some(pos) if is_inside_world_limits(pos.0, pos.1) => positions.push(pos),
            Some(_) => return Ok(None),
            None => return Err(ApiError::bad_request("invalid position")),
        }
    }
    if let Some(ps) = &q.positions {
        for s in ps {
            match parse_position(s) {
                Some(pos) if is_inside_world_limits(pos.0, pos.1) => positions.push(pos),
                Some(_) => return Ok(None),
                None => return Err(ApiError::bad_request("invalid position in positions[]")),
            }
        }
    }

    let mut places_ids = q.places_ids.clone().unwrap_or_default();
    places_ids.extend(body_place_ids);

    let community_id = q.community_id.clone().or(body_community_id);

    let from = q
        .from
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    let to =
        q.to.as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc));

    let search = match q.search.as_deref() {
        Some(s) if !has_three_word_chars(s) => return Ok(None),
        Some(s) => Some(s.to_string()),
        None => None,
    };

    let owner = resolve_owner(q);

    let creator = if owner {
        None
    } else {
        match q.creator.as_deref() {
            Some(c) if is_ethereum_address(c) => Some(c.to_lowercase()),
            Some(_) => return Ok(None),
            None => None,
        }
    };

    let estate_id = match q.estate_id.as_deref() {
        Some(e) if !e.is_empty() => {
            if e.parse::<f64>().map(|n| n.is_finite()).unwrap_or(false) {
                Some(e.to_string())
            } else {
                return Ok(None);
            }
        }
        _ => None,
    };

    Ok(Some(EventListFilters {
        limit,
        offset,
        list,
        order,
        highlighted,
        creator,
        world: q.world.as_deref().and_then(parse_bool),
        world_names: q.world_names.clone().unwrap_or_default(),
        positions,
        estate_id,
        community_id,
        places_ids,
        from,
        to,
        search,
        user: user.clone(),
        rejected: if admin {
            q.rejected.as_deref().and_then(parse_bool)
        } else {
            None
        },
        approved: if admin {
            q.approved.as_deref().and_then(parse_bool)
        } else {
            None
        },
        deleted: if admin {
            q.deleted.as_deref().and_then(parse_bool)
        } else {
            None
        },
        admin,
        only_attendee: resolve_only_attendee(q) && user.is_some(),
        owner,
    }))
}

fn has_three_word_chars(s: &str) -> bool {
    let mut run = 0;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn is_ethereum_address(s: &str) -> bool {
    let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) else {
        return false;
    };
    hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn is_inside_world_limits(x: i32, y: i32) -> bool {
    const RANGES: [(i32, i32, i32, i32); 4] = [
        (-150, -150, 150, 150),
        (62, 151, 162, 158),
        (151, 144, 162, 150),
        (151, 59, 163, 143),
    ];
    RANGES.iter().any(|(x_min, y_min, x_max, y_max)| {
        x >= *x_min && x <= *x_max && y >= *y_min && y <= *y_max
    })
}

fn resolve_only_attendee(q: &EventListQuery) -> bool {
    match q.only_attendee.as_deref() {
        Some(v) => parse_bool(v).unwrap_or(true),
        None => false,
    }
}

fn resolve_owner(q: &EventListQuery) -> bool {
    q.owner.as_deref().and_then(parse_bool).unwrap_or(false)
}

enum EventLocation {
    World(String),
    Place(String),
}

fn event_location(world: bool, server: Option<&str>, x: i32, y: i32) -> Option<EventLocation> {
    if world {
        server
            .filter(|s| !s.is_empty())
            .map(|s| EventLocation::World(s.to_string()))
    } else {
        Some(EventLocation::Place(format!("{},{}", x, y)))
    }
}

fn connected_key(world: bool, server: Option<&str>, x: i32, y: i32) -> String {
    match server {
        Some(s) if world && !s.is_empty() => s.to_string(),
        _ => format!("{},{}", x, y),
    }
}

async fn attach_connected_users(state: &AppState, events: &mut [EventRecord]) {
    use std::collections::{HashMap, HashSet};

    if events.is_empty() {
        return;
    }

    let mut worlds: HashSet<String> = HashSet::new();
    let mut places: HashSet<String> = HashSet::new();
    for e in events.iter() {
        match event_location(e.world, e.server.as_deref(), e.x, e.y) {
            Some(EventLocation::World(w)) => {
                worlds.insert(w);
            }
            Some(EventLocation::Place(p)) => {
                places.insert(p);
            }
            None => {}
        }
    }

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for w in worlds {
        let addrs = state.comms.get_world_participants(&w).await;
        map.insert(w, addrs);
    }
    for p in places {
        let addrs = state.comms.get_scene_participants(&p).await;
        map.insert(p, addrs);
    }

    for e in events.iter_mut() {
        let key = connected_key(e.world, e.server.as_deref(), e.x, e.y);
        e.connected_addresses = Some(map.get(&key).cloned().unwrap_or_default());
    }
}

async fn optional_user(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Option<String>, ApiError> {
    Ok(crate::auth_chain::optional_signer(headers, method, path)
        .await?
        .map(|s| s.as_str().to_string()))
}

fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub(crate) fn is_admin_request(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(token) = state.admin_token.as_deref().filter(|t| !t.is_empty()) else {
        return false;
    };
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|presented| timing_safe_eq(presented, token))
        .unwrap_or(false)
}

fn with_connected(q: &EventListQuery) -> bool {
    q.with_connected_users
        .as_deref()
        .and_then(parse_bool)
        .unwrap_or(false)
}

#[utoipa::path(
    get,
    path = "/api/events",
    tag = "events",
    params(
        ("list" = Option<String>, Query),
        ("search" = Option<String>, Query),
        ("limit" = Option<i64>, Query),
        ("offset" = Option<i64>, Query),
        ("order" = Option<String>, Query),
        ("highlighted" = Option<String>, Query),
        ("creator" = Option<String>, Query),
        ("owner" = Option<String>, Query),
        ("only_attendee" = Option<String>, Query),
        ("world" = Option<String>, Query),
        ("world_names" = Option<Vec<String>>, Query),
        ("position" = Option<String>, Query),
        ("positions" = Option<Vec<String>>, Query),
        ("estate_id" = Option<String>, Query),
        ("community_id" = Option<String>, Query),
        ("places_ids" = Option<Vec<String>>, Query),
        ("from" = Option<String>, Query),
        ("to" = Option<String>, Query),
        ("schedule" = Option<String>, Query),
        ("with_connected_users" = Option<String>, Query),
        ("approved" = Option<String>, Query),
        ("rejected" = Option<String>, Query)
    ),
    responses(
        (status = 200, body = ApiOk<EventListData>),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn get_event_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiOk<EventListData>>, ApiError> {
    let q = EventListQuery::from_pairs(&pairs);
    let user = optional_user(&headers, "get", "/api/events").await?;
    if q.only_attendee.is_some() && user.is_none() {
        return Err(ApiError::unauthorized(
            "only_attendee filter requieres autentication",
        ));
    }
    if resolve_owner(&q) && user.is_none() {
        return Err(ApiError::unauthorized(
            "owner filter requires authentication",
        ));
    }
    let admin = is_admin_request(&headers, &state);
    let Some(filters) = parse_filters(&q, Vec::new(), None, user, admin)? else {
        return Ok(Json(ApiOk::new(EventListData::Events(Vec::new()))));
    };
    let envelope_with_total = !filters.places_ids.is_empty() || filters.community_id.is_some();
    let (mut events, total) = state.events.query(&filters, envelope_with_total).await?;
    if with_connected(&q) {
        attach_connected_users(&state, &mut events).await;
    }
    let data = if envelope_with_total {
        EventListData::WithTotal(EventListWithTotal { events, total })
    } else {
        EventListData::Events(events)
    };
    Ok(Json(ApiOk::new(data)))
}

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
pub struct EventSearchBody {
    #[serde(default, rename = "placeIds")]
    pub place_ids: Vec<String>,
    #[serde(default, rename = "communityId")]
    pub community_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/events/search",
    tag = "events",
    request_body = EventSearchBody,
    params(
        ("list" = Option<String>, Query),
        ("search" = Option<String>, Query),
        ("limit" = Option<i64>, Query),
        ("offset" = Option<i64>, Query),
        ("only_attendee" = Option<String>, Query),
        ("owner" = Option<String>, Query),
        ("with_connected_users" = Option<String>, Query)
    ),
    responses(
        (status = 200, body = ApiOk<EventListData>),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn post_event_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
    Json(body): Json<EventSearchBody>,
) -> Result<Json<ApiOk<EventListData>>, ApiError> {
    let q = EventListQuery::from_pairs(&pairs);
    let user = optional_user(&headers, "post", "/api/events/search").await?;
    if q.only_attendee.is_some() && user.is_none() {
        return Err(ApiError::unauthorized(
            "only_attendee filter requieres autentication",
        ));
    }
    if resolve_owner(&q) && user.is_none() {
        return Err(ApiError::unauthorized(
            "owner filter requires authentication",
        ));
    }
    let admin = is_admin_request(&headers, &state);
    let Some(filters) = parse_filters(&q, body.place_ids, body.community_id, user, admin)? else {
        return Ok(Json(ApiOk::new(EventListData::Events(Vec::new()))));
    };
    let envelope_with_total = !filters.places_ids.is_empty() || filters.community_id.is_some();
    let (mut events, total) = state.events.query(&filters, envelope_with_total).await?;
    if with_connected(&q) {
        attach_connected_users(&state, &mut events).await;
    }
    let data = if envelope_with_total {
        EventListData::WithTotal(EventListWithTotal { events, total })
    } else {
        EventListData::Events(events)
    };
    Ok(Json(ApiOk::new(data)))
}

#[utoipa::path(
    get,
    path = "/api/events/{event_id}",
    tag = "events",
    params(("event_id" = String, Path)),
    responses(
        (status = 200, body = ApiOk<EventRecord>),
        (status = 404, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn get_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Result<Json<ApiOk<EventRecord>>, ApiError> {
    let mut evt = state
        .events
        .get(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Not found event \"{}\"", event_id)))?;
    if !evt.approved {
        return Err(ApiError::not_found(format!(
            "Not found event \"{}\"",
            event_id
        )));
    }
    let path = format!("/api/events/{}", event_id);
    if let Some(user) = optional_user(&headers, "get", &path).await? {
        evt.attending = state.events.is_user_attending(&event_id, &user).await?;
    }
    Ok(Json(ApiOk::new(evt)))
}

#[utoipa::path(
    get,
    path = "/api/events/attending",
    tag = "events",
    responses(
        (status = 200, body = ApiOk<Vec<EventRecord>>),
        (status = 401, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn get_attending_event_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiOk<Vec<EventRecord>>>, ApiError> {
    let user = optional_user(&headers, "get", "/api/events/attending")
        .await?
        .ok_or_else(|| ApiError::unauthorized("Unauthorized"))?;
    let events = state.events.attending(&user).await?;
    Ok(Json(ApiOk::new(events)))
}

#[derive(Debug, Deserialize, Default)]
pub struct ModerationListQuery {
    pub limit: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/events/moderation",
    tag = "events",
    params(("limit" = Option<i64>, Query)),
    responses(
        (status = 200, body = ApiOk<Vec<EventRecord>>),
        (status = 403, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn get_moderation_list(
    State(state): State<AppState>,
    _admin: crate::admin::RequireAdmin,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiOk<Vec<EventRecord>>>, ApiError> {
    let q = ModerationListQuery {
        limit: pairs
            .iter()
            .find(|(k, _)| k == "limit")
            .map(|(_, v)| v.clone()),
    };
    let limit = q
        .limit
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .map(|n| n.clamp(0, 500))
        .unwrap_or(24);
    let events = state.events.moderation_pending(limit).await?;
    Ok(Json(ApiOk::new(events)))
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    /// A community_id query selects the WithTotal envelope. The Unity client
    /// (EventWithPlaceIdDTOListResponse) parses exactly
    /// `{"ok":true,"data":{"events":[...],"total":n}}` for this query, so the
    /// empty case must serialize to that byte sequence -- the untagged
    /// EventListData must not collapse to a bare array.
    #[test]
    fn empty_community_query_serializes_prod_envelope() {
        let pairs = vec![
            (
                "community_id".to_string(),
                "11111111-2222-3333-4444-555555555555".to_string(),
            ),
            ("limit".to_string(), "10".to_string()),
            ("offset".to_string(), "0".to_string()),
        ];
        let q = EventListQuery::from_pairs(&pairs);
        let filters = parse_filters(&q, Vec::new(), None, Some("0xabc".to_string()), false)
            .expect("community query parses")
            .expect("community query is servable");
        // The handler keys the WithTotal envelope off this condition.
        assert!(!filters.places_ids.is_empty() || filters.community_id.is_some());

        let body = ApiOk::new(EventListData::WithTotal(EventListWithTotal {
            events: Vec::new(),
            total: 0,
        }));
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"ok":true,"data":{"events":[],"total":0}}"#
        );
    }

    /// The same query without community_id serializes the flat list envelope
    /// (EventDTOListResponse: `{"ok":true,"data":[...]}`).
    #[test]
    fn plain_query_serializes_flat_list() {
        let pairs = vec![("limit".to_string(), "10".to_string())];
        let q = EventListQuery::from_pairs(&pairs);
        let filters = parse_filters(&q, Vec::new(), None, None, false)
            .expect("plain query parses")
            .expect("plain query is servable");
        assert!(filters.places_ids.is_empty() && filters.community_id.is_none());

        let body = ApiOk::new(EventListData::Events(Vec::new()));
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"ok":true,"data":[]}"#
        );
    }
}
