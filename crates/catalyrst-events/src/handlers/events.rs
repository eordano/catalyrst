use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth_chain::try_extract_signer;
use crate::http::response::{ApiError, ApiOk};
use crate::ports::events::{EventListFilters, EventListType, SortOrder};
use crate::schemas::EventRecord;
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventListWithTotal {
    pub events: Vec<EventRecord>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub enum EventListData {
    WithTotal(EventListWithTotal),
    Events(Vec<EventRecord>),
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventUpsertResult {
    pub id: String,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub local: Value,
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
    let limit = q
        .limit
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .map(|n| n.clamp(0, 500))
        .unwrap_or(500);
    let offset = q
        .offset
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .map(|n| n.max(0))
        .unwrap_or(0);
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

    let from = q
        .from
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));
    let to =
        q.to.as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc));

    let search = q.search.as_deref().and_then(|s| {
        if s.len() >= 3 {
            Some(s.to_string())
        } else {
            None
        }
    });

    let owner = resolve_owner(q);

    Ok(EventListFilters {
        limit,
        offset,
        list,
        order,
        highlighted,
        creator: if owner {
            None
        } else {
            q.creator.as_ref().map(|c| c.to_lowercase())
        },
        world: q.world.as_deref().and_then(parse_bool),
        world_names: q.world_names.clone().unwrap_or_default(),
        positions,
        estate_id: q.estate_id.clone(),
        community_id,
        places_ids,
        from,
        to,
        search,
        user: user.clone(),
        rejected: q.rejected.as_deref().and_then(parse_bool),
        only_attendee: resolve_only_attendee(q) && user.is_some(),
        owner,
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

fn optional_user(headers: &HeaderMap, method: &str, path: &str) -> Option<String> {
    try_extract_signer(headers, method, path).map(|s| s.to_lowercase())
}

fn with_connected(q: &EventListQuery) -> bool {
    q.with_connected_users
        .as_deref()
        .and_then(parse_bool)
        .unwrap_or(false)
}

pub async fn get_event_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiOk<EventListData>>, ApiError> {
    let q = EventListQuery::from_pairs(&pairs);
    let user = optional_user(&headers, "get", "/api/events");
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
    let filters = parse_filters(&q, Vec::new(), None, user)?;
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
) -> Result<Json<ApiOk<EventListData>>, ApiError> {
    let q = EventListQuery::from_pairs(&pairs);
    let user = optional_user(&headers, "post", "/api/events/search");
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
    let filters = parse_filters(&q, body.place_ids, body.community_id, user)?;
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

#[derive(Debug, Deserialize, Default)]
pub struct ModerationListQuery {
    pub limit: Option<String>,
}

pub async fn get_moderation_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiOk<Vec<EventRecord>>>, ApiError> {
    crate::admin::authorize_admin(&state, &headers)?;
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

#[derive(Debug, Deserialize)]
pub struct CreateEventBody {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub start_at: Option<String>,
    #[serde(default)]
    pub finish_at: Option<String>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,

    #[serde(default)]
    pub id: Option<String>,
}

fn derive_event_id(name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    name.hash(&mut h);
    Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_default()
        .hash(&mut h);
    format!("local-{:016x}", h.finish())
}

pub async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateEventBody>,
) -> Result<Json<ApiOk<EventUpsertResult>>, ApiError> {
    crate::admin::authorize_admin(&state, &headers)?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    let event_id = body
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| derive_event_id(name));

    let doc = json!({
        "action": "create",
        "name": name,
        "description": body.description,
        "start_at": body.start_at,
        "finish_at": body.finish_at,
        "x": body.x,
        "y": body.y,
        "approved": false,
        "created_via": "admin",
        "moderated_at": Utc::now().to_rfc3339(),
    });

    let merged = state.events.upsert_local(&event_id, "admin", doc).await?;
    Ok(Json(ApiOk::new(EventUpsertResult {
        id: event_id,
        local: merged,
    })))
}

#[derive(Debug, Deserialize)]
pub struct PatchEventBody {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub approved: Option<bool>,
    #[serde(default)]
    pub rejected: Option<bool>,
    #[serde(default)]
    pub highlighted: Option<bool>,
    #[serde(default)]
    pub trending: Option<bool>,

    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn patch_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
    Json(body): Json<PatchEventBody>,
) -> Result<Json<ApiOk<EventUpsertResult>>, ApiError> {
    crate::admin::authorize_admin(&state, &headers)?;

    let known =
        state.events.exists(&event_id).await? || state.events.get_local(&event_id).await?.is_some();
    if !known {
        return Err(ApiError::not_found(format!(
            "Not found event \"{}\"",
            event_id
        )));
    }

    let mut doc = serde_json::Map::new();

    match body.action.as_deref() {
        Some("approve") => {
            doc.insert("approved".into(), json!(true));
            doc.insert("rejected".into(), json!(false));
        }
        Some("reject") | Some("archive") => {
            doc.insert("approved".into(), json!(false));
            doc.insert("rejected".into(), json!(true));
        }
        Some("feature") => {
            doc.insert("highlighted".into(), json!(true));
        }
        Some("unfeature") => {
            doc.insert("highlighted".into(), json!(false));
        }
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "unknown action \"{}\" (expected approve|reject|feature|unfeature|archive)",
                other
            )));
        }
        None => {}
    }

    if let Some(v) = body.approved {
        doc.insert("approved".into(), json!(v));
    }
    if let Some(v) = body.rejected {
        doc.insert("rejected".into(), json!(v));
    }
    if let Some(v) = body.highlighted {
        doc.insert("highlighted".into(), json!(v));
    }
    if let Some(v) = body.trending {
        doc.insert("trending".into(), json!(v));
    }
    if let Some(v) = &body.name {
        doc.insert("name".into(), json!(v));
    }
    if let Some(v) = &body.description {
        doc.insert("description".into(), json!(v));
    }

    if doc.is_empty() {
        return Err(ApiError::bad_request(
            "no moderation fields provided (action / approved / rejected / highlighted / trending / name / description)",
        ));
    }
    doc.insert("moderated_at".into(), json!(Utc::now().to_rfc3339()));

    let merged = state
        .events
        .upsert_local(&event_id, "admin", Value::Object(doc))
        .await?;
    Ok(Json(ApiOk::new(EventUpsertResult {
        id: event_id,
        local: merged,
    })))
}

pub async fn delete_event(Path(_event_id): Path<String>) -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "event deletion is handled via the federation write path",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(v: Option<&str>) -> EventListQuery {
        EventListQuery {
            only_attendee: v.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn only_attendee_present_resolves_true_unless_false() {
        assert!(!resolve_only_attendee(&q(None)));
        assert!(resolve_only_attendee(&q(Some("true"))));
        assert!(resolve_only_attendee(&q(Some("1"))));
        assert!(!resolve_only_attendee(&q(Some("false"))));
        assert!(!resolve_only_attendee(&q(Some("0"))));
        assert!(resolve_only_attendee(&q(Some("yes"))));
    }

    fn owner_q(v: Option<&str>) -> EventListQuery {
        EventListQuery {
            owner: v.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn owner_resolves_only_for_explicit_truthy() {
        assert!(!resolve_owner(&owner_q(None)));
        assert!(resolve_owner(&owner_q(Some("true"))));
        assert!(resolve_owner(&owner_q(Some("1"))));
        assert!(!resolve_owner(&owner_q(Some("false"))));
        assert!(!resolve_owner(&owner_q(Some("0"))));
        assert!(!resolve_owner(&owner_q(Some("yes"))));
    }

    #[test]
    fn parse_filters_owner_suppresses_creator() {
        let q = EventListQuery {
            owner: Some("true".into()),
            creator: Some("0xDEF".into()),
            ..Default::default()
        };
        let f = parse_filters(&q, Vec::new(), None, Some("0xabc".into())).unwrap();
        assert!(f.owner);
        assert!(f.creator.is_none(), "creator must be dropped under owner");

        let q2 = EventListQuery {
            creator: Some("0xDEF".into()),
            ..Default::default()
        };
        let f2 = parse_filters(&q2, Vec::new(), None, Some("0xabc".into())).unwrap();
        assert!(!f2.owner);
        assert_eq!(f2.creator.as_deref(), Some("0xdef"));
    }

    #[test]
    fn connected_location_and_key() {
        assert!(matches!(
            event_location(true, Some("my.dcl.eth"), 0, 0),
            Some(EventLocation::World(ref w)) if w.as_str() == "my.dcl.eth"
        ));
        assert_eq!(connected_key(true, Some("my.dcl.eth"), 0, 0), "my.dcl.eth");

        assert!(matches!(
            event_location(false, None, 10, 20),
            Some(EventLocation::Place(ref p)) if p.as_str() == "10,20"
        ));
        assert_eq!(connected_key(false, None, 10, 20), "10,20");

        assert!(event_location(true, None, 5, 6).is_none());
        assert!(event_location(true, Some(""), 5, 6).is_none());
        assert_eq!(connected_key(true, None, 5, 6), "5,6");
    }

    fn sample_event(connected: Option<Vec<String>>) -> EventRecord {
        let ts = |s: &str| {
            DateTime::parse_from_rfc3339(s)
                .expect("valid rfc3339")
                .with_timezone(&Utc)
        };
        EventRecord {
            id: "409b6eb1-1fe2-40ab-afc3-6e117dd8cf10".into(),
            name: "Community Meeting".into(),
            image: Some("https://events-assets/poster/6f0d.png".into()),
            image_vertical: None,
            description: None,
            start_at: Some(ts("2026-07-06T14:00:00Z")),
            finish_at: Some(ts("2026-07-06T15:00:00Z")),
            next_start_at: None,
            next_finish_at: None,
            duration: Some(3_600_000),
            all_day: false,
            x: 10,
            y: -20,
            server: None,
            url: Some("https://decentraland.org/play/?position=10%2C-20".into()),
            user: Some("0xc073b6a602d4061d57ba78db3d93a3f856476396".into()),
            user_name: None,
            estate_id: Some("1164".into()),
            estate_name: Some("Genesis Plaza".into()),
            scene_name: None,
            approved: true,
            rejected: false,
            highlighted: false,
            trending: false,
            world: false,
            recurrent: true,
            recurrent_frequency: Some("MONTHLY".into()),
            recurrent_weekday_mask: 0,
            recurrent_month_mask: 0,
            recurrent_interval: 1,
            recurrent_setpos: None,
            recurrent_monthday: Some(6),
            recurrent_count: None,
            recurrent_until: Some(ts("2026-12-06T14:00:00Z")),
            recurrent_dates: vec![ts("2026-07-06T14:00:00Z"), ts("2026-08-06T14:00:00Z")],
            categories: vec!["social".into()],
            schedules: vec![],
            total_attendees: 3,
            latest_attendees: vec!["0xeb0c682e1ca11e62eabe436ea36459d57616e5f1".into()],
            coordinates: [10, -20],
            position: [10, -20],
            live: false,
            attending: false,
            place_id: None,
            community_id: Some("e99471aa-31c4-4952-abf6-99905445f43b".into()),
            connected_addresses: connected,
        }
    }

    #[test]
    fn wire_identity_event_list_plain() {
        let events = vec![sample_event(None), sample_event(Some(vec!["0x1".into()]))];
        let new = serde_json::to_value(ApiOk::new(EventListData::Events(events.clone()))).unwrap();
        assert_eq!(new, json!({"ok": true, "data": events}));

        assert!(new["data"][0].get("connected_addresses").is_none());
        assert!(new["data"][1]["connected_addresses"].is_array());
        assert!(new["data"][0]["description"].is_null());
    }

    #[test]
    fn wire_identity_event_list_with_total() {
        let events = vec![sample_event(None)];
        let new = serde_json::to_value(ApiOk::new(EventListData::WithTotal(EventListWithTotal {
            events: events.clone(),
            total: 42,
        })))
        .unwrap();
        assert_eq!(
            new,
            json!({"ok": true, "data": {"events": events, "total": 42}})
        );
    }

    #[test]
    fn wire_identity_moderation_list() {
        let events: Vec<EventRecord> = vec![sample_event(None)];
        let new = serde_json::to_value(ApiOk::new(events.clone())).unwrap();
        assert_eq!(new, json!({"ok": true, "data": events}));

        let empty: Vec<EventRecord> = Vec::new();
        let new = serde_json::to_value(ApiOk::new(empty.clone())).unwrap();
        assert_eq!(new, json!({"ok": true, "data": empty}));
    }

    #[test]
    fn wire_identity_event_upsert_result() {
        let merged = json!({
            "action": "create",
            "name": "Community Meeting",
            "description": null,
            "approved": false,
            "created_via": "admin",
            "some_future_key": {"nested": [1, 2, null]}
        });
        let new = serde_json::to_value(ApiOk::new(EventUpsertResult {
            id: "local-0011223344556677".into(),
            local: merged.clone(),
        }))
        .unwrap();
        assert_eq!(
            new,
            json!({"ok": true, "data": {"id": "local-0011223344556677", "local": merged}})
        );
        assert!(new["data"]["local"]["description"].is_null());
        assert_eq!(
            new["data"]["local"]["some_future_key"]["nested"][2],
            json!(null)
        );
    }
}
