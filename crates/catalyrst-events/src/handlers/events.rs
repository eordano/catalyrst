use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth_chain::try_extract_signer;
use crate::http::response::{ApiError, ApiOk};
use crate::ports::events::{EventListFilters, EventListType, SortOrder};
use crate::schemas::EventRecord;
use crate::AppState;

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
}

impl EventListQuery {
    /// Build from raw query pairs. serde_urlencoded (axum `Query<T>`) cannot
    /// deserialize repeated keys into a `Vec<String>` field, so the array params
    /// the real events client sends (`positions=X&positions=Y`, places_ids,
    /// world_names — see decentraland/events Places.ts `query.append("positions", …)`)
    /// previously 400'd ("expected a sequence") and the filters were unusable.
    /// Extract via `Query<Vec<(String,String)>>` and collect repeated keys here.
    /// Also accept the `key[]` bracket alias for clients that use it.
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
) -> Result<EventListFilters, ApiError> {
    let limit = q.limit.as_deref().and_then(|s| s.parse::<i64>().ok()).map(|n| n.clamp(0, 500)).unwrap_or(500);
    let offset = q.offset.as_deref().and_then(|s| s.parse::<i64>().ok()).map(|n| n.max(0)).unwrap_or(0);
    let order = match q.order.as_deref() {
        Some("desc") => SortOrder::Desc,
        _ => SortOrder::Asc,
    };
    let mut list = match q.list.as_deref() {
        Some("all") => EventListType::All,
        Some("live") => EventListType::Live,
        Some("upcoming") => EventListType::Upcoming,
        Some("highlight") => EventListType::Active,
        _ => EventListType::Active,
    };
    let mut highlighted = q.highlighted.as_deref().and_then(parse_bool);
    if matches!(q.list.as_deref(), Some("highlight")) {
        highlighted = Some(true);
        list = EventListType::Active;
    }

    let mut positions: Vec<(i32, i32)> = Vec::new();
    if let Some(p) = &q.position {
        if let Some(pos) = parse_position(p) {
            positions.push(pos);
        } else {
            return Err(ApiError::bad_request("invalid position"));
        }
    }
    if let Some(ps) = &q.positions {
        for s in ps {
            if let Some(pos) = parse_position(s) {
                positions.push(pos);
            } else {
                return Err(ApiError::bad_request("invalid position in positions[]"));
            }
        }
    }

    let mut places_ids = q.places_ids.clone().unwrap_or_default();
    places_ids.extend(body_place_ids);

    let community_id = q.community_id.clone().or(body_community_id);

    let from = q.from.as_deref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Utc));
    let to = q.to.as_deref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Utc));

    let search = q.search.as_deref().and_then(|s| if s.len() >= 3 { Some(s.to_string()) } else { None });

    Ok(EventListFilters {
        limit,
        offset,
        list,
        order,
        highlighted,
        creator: q.creator.as_ref().map(|c| c.to_lowercase()),
        world: q.world.as_deref().and_then(parse_bool),
        world_names: q.world_names.clone().unwrap_or_default(),
        positions,
        estate_id: q.estate_id.clone(),
        community_id,
        places_ids,
        from,
        to,
        search,
        user,
        rejected: q.rejected.as_deref().and_then(parse_bool),
    })
}

fn optional_user(headers: &HeaderMap, method: &str, path: &str) -> Option<String> {
    try_extract_signer(headers, method, path).map(|s| s.to_lowercase())
}

fn with_connected(q: &EventListQuery) -> bool {
    q.with_connected_users.as_deref().and_then(parse_bool).unwrap_or(false)
}

pub async fn get_event_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<Value>, ApiError> {
    let q = EventListQuery::from_pairs(&pairs);
    let user = optional_user(&headers, "get", "/api/events");
    let filters = parse_filters(&q, Vec::new(), None, user)?;
    let envelope_with_total = !filters.places_ids.is_empty() || filters.community_id.is_some();
    let (mut events, total) = state.events.query(&filters, envelope_with_total).await?;
    if with_connected(&q) {
        state.events.attach_connected_addresses(&mut events).await?;
    }
    if envelope_with_total {
        Ok(Json(json!({"ok": true, "data": {"events": events, "total": total}})))
    } else {
        Ok(Json(json!({"ok": true, "data": events})))
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct EventSearchBody {
    #[serde(default, rename = "placeIds")]
    pub place_ids: Vec<String>,
    #[serde(default, rename = "communityId")]
    pub community_id: Option<String>,
}

pub async fn post_event_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
    Json(body): Json<EventSearchBody>,
) -> Result<Json<Value>, ApiError> {
    let q = EventListQuery::from_pairs(&pairs);
    let user = optional_user(&headers, "post", "/api/events/search");
    let filters = parse_filters(&q, body.place_ids, body.community_id, user)?;
    let envelope_with_total = !filters.places_ids.is_empty() || filters.community_id.is_some();
    let (mut events, total) = state.events.query(&filters, envelope_with_total).await?;
    if with_connected(&q) {
        state.events.attach_connected_addresses(&mut events).await?;
    }
    if envelope_with_total {
        Ok(Json(json!({"ok": true, "data": {"events": events, "total": total}})))
    } else {
        Ok(Json(json!({"ok": true, "data": events})))
    }
}

pub async fn get_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Result<Json<ApiOk<EventRecord>>, ApiError> {
    let mut evt = state.events.get(&event_id).await?
        .ok_or_else(|| ApiError::not_found(format!("Not found event \"{}\"", event_id)))?;
    if !evt.approved {
        return Err(ApiError::not_found(format!("Not found event \"{}\"", event_id)));
    }
    let path = format!("/api/events/{}", event_id);
    if let Some(user) = optional_user(&headers, "get", &path) {
        evt.attending = state.events.is_user_attending(&event_id, &user).await?;
    }
    Ok(Json(ApiOk::new(evt)))
}

pub async fn get_attending_event_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiOk<Vec<EventRecord>>>, ApiError> {
    let user = optional_user(&headers, "get", "/api/events/attending")
        .ok_or_else(|| ApiError::unauthorized("Unauthorized"))?;
    let events = state.events.attending(&user).await?;
    Ok(Json(ApiOk::new(events)))
}

pub async fn create_event() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /api/events is a federation-signed action (EventCreate per docs/federation/events.md §3); writes will land with the federation phase",
    ))
}

pub async fn patch_event() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "PATCH /api/events/{id} is a federation-signed action (EventModerate per docs/federation/events.md §3); writes will land with the federation phase",
    ))
}
