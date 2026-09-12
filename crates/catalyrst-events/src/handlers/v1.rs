use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::handlers::events::{list_events, load_event_for_viewer, optional_user, EventListData};
use crate::http::response::{ApiError, ApiErrorBody, ApiOk};
use crate::ports::events::{EventListFilters, EventListType, SortOrder};
use crate::schemas::EventRecord;
use crate::AppState;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct V1EventRecord {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub event: EventRecord,
    pub destination_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown> | null"))]
    pub destination: Option<Value>,
}

pub fn is_uuid(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

pub fn destination_id(event: &EventRecord) -> Option<String> {
    event
        .place_id
        .clone()
        .filter(|id| !id.is_empty())
        .or_else(|| {
            event
                .world
                .then(|| event.server.clone())
                .flatten()
                .filter(|s| !s.is_empty())
        })
}

fn is_live_at(event: &EventRecord, now: DateTime<Utc>) -> bool {
    event.next_start_at.is_some_and(|start| start <= now)
        && event.next_finish_at.is_some_and(|finish| finish >= now)
}

fn start_key(event: &EventRecord) -> i64 {
    event
        .next_start_at
        .or(event.start_at)
        .map(|t| t.timestamp_millis())
        .unwrap_or(i64::MAX)
}

pub fn order_destination_events(events: &mut [EventRecord], now: DateTime<Utc>) {
    events.sort_by(|a, b| {
        let (live_a, live_b) = (is_live_at(a, now), is_live_at(b, now));
        live_b
            .cmp(&live_a)
            .then_with(|| start_key(a).cmp(&start_key(b)))
    });
}

#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "v1",
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
pub async fn get_v1_event_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiOk<EventListData>>, ApiError> {
    list_events(&state, &headers, &pairs, "/v1/events").await
}

#[utoipa::path(
    get,
    path = "/v1/events/{event_id}",
    tag = "v1",
    params(("event_id" = String, Path)),
    responses(
        (status = 200, body = ApiOk<V1EventRecord>),
        (status = 400, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn get_v1_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Result<Json<ApiOk<V1EventRecord>>, ApiError> {
    let path = format!("/v1/events/{}", event_id);
    if !is_uuid(&event_id) {
        return Err(ApiError::not_found(format!("Event not found: {event_id}")));
    }
    let event = load_event_for_viewer(&state, &headers, &event_id, &path).await?;
    let destination_id = destination_id(&event);
    let destination = match &destination_id {
        Some(id) => state.places.get_destination(id, &headers).await,
        None => None,
    };
    Ok(Json(ApiOk::new(V1EventRecord {
        event,
        destination_id,
        destination,
    })))
}

#[utoipa::path(
    get,
    path = "/v1/destinations/{id}/events",
    tag = "v1",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = ApiOk<Vec<EventRecord>>),
        (status = 400, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn get_v1_destination_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiOk<Vec<EventRecord>>>, ApiError> {
    let path = format!("/v1/destinations/{}/events", id);
    let viewer = optional_user(&headers, "get", &path).await?;
    let filters = EventListFilters {
        limit: 500,
        offset: 0,
        list: EventListType::Active,
        order: SortOrder::Asc,
        destination_ids: vec![id.to_lowercase()],
        user: viewer,
        include_own: true,
        ..Default::default()
    };
    let (mut events, _) = state.events.query(&filters, false).await?;
    order_destination_events(&mut events, Utc::now());
    Ok(Json(ApiOk::new(events)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, 1, hour, 0, 0).unwrap()
    }

    fn event(id: &str, start: Option<u32>, finish: Option<u32>) -> EventRecord {
        EventRecord {
            id: id.to_string(),
            name: id.to_string(),
            image: None,
            image_vertical: None,
            description: None,
            start_at: start.map(at),
            finish_at: finish.map(at),
            next_start_at: start.map(at),
            next_finish_at: finish.map(at),
            duration: Some(0),
            all_day: false,
            x: 0,
            y: 0,
            server: None,
            url: None,
            user: None,
            user_name: None,
            estate_id: None,
            estate_name: None,
            scene_name: None,
            approved: true,
            rejected: false,
            highlighted: false,
            trending: false,
            world: false,
            recurrent: false,
            recurrent_frequency: None,
            recurrent_weekday_mask: 0,
            recurrent_month_mask: 0,
            recurrent_interval: 1,
            recurrent_setpos: None,
            recurrent_monthday: None,
            recurrent_count: None,
            recurrent_until: None,
            recurrent_dates: vec![],
            categories: vec![],
            schedules: vec![],
            total_attendees: 0,
            latest_attendees: vec![],
            coordinates: [0, 0],
            position: [0, 0],
            live: false,
            attending: false,
            place_id: None,
            community_id: None,
            featured_item: None,
            created_at: None,
            updated_at: None,
            approved_by: None,
            rejected_by: None,
            rejection_reason: None,
            deleted_by_user: false,
            deleted_by_admin: false,
            deleted_by: None,
            deleted_at: None,
            deleted_reason: None,
            previous_place_id: None,
            connected_addresses: None,
        }
    }

    fn ids(events: &[EventRecord]) -> Vec<&str> {
        events.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn live_events_come_first_then_the_soonest_upcoming() {
        let now = at(12);
        let mut events = vec![
            event("later", Some(18), Some(19)),
            event("soon", Some(14), Some(15)),
            event("live-late", Some(11), Some(13)),
            event("live-early", Some(9), Some(13)),
        ];
        order_destination_events(&mut events, now);
        assert_eq!(ids(&events), ["live-early", "live-late", "soon", "later"]);
    }

    #[test]
    fn an_occurrence_ending_exactly_now_is_still_live() {
        let now = at(12);
        let mut events = vec![
            event("next", Some(13), Some(14)),
            event("edge", Some(10), Some(12)),
        ];
        order_destination_events(&mut events, now);
        assert_eq!(ids(&events), ["edge", "next"]);
    }

    #[test]
    fn ties_keep_the_repository_order_and_undated_rows_sort_last() {
        let now = at(12);
        let mut events = vec![
            event("b", Some(14), Some(15)),
            event("a", Some(14), Some(15)),
            event("undated", None, None),
        ];
        order_destination_events(&mut events, now);
        assert_eq!(ids(&events), ["b", "a", "undated"]);
        let mut events = vec![
            event("undated", None, None),
            event("dated", Some(14), Some(15)),
        ];
        order_destination_events(&mut events, now);
        assert_eq!(ids(&events), ["dated", "undated"]);
    }

    #[test]
    fn the_destination_id_is_the_wire_place_id_for_both_kinds() {
        let mut e = event("e", None, None);
        assert_eq!(destination_id(&e), None);
        e.place_id = Some("".into());
        assert_eq!(destination_id(&e), None);
        e.place_id = Some("my-world.dcl.eth".into());
        assert_eq!(destination_id(&e).as_deref(), Some("my-world.dcl.eth"));
    }

    #[test]
    fn a_locally_written_world_event_names_its_world_through_server() {
        let mut e = event("e", None, None);
        e.world = true;
        e.server = Some("my-world.dcl.eth".into());
        assert_eq!(destination_id(&e).as_deref(), Some("my-world.dcl.eth"));
        e.server = Some("".into());
        assert_eq!(destination_id(&e), None);
        let mut genesis = event("g", None, None);
        genesis.world = false;
        genesis.server = Some("x".into());
        assert_eq!(destination_id(&genesis), None);
        let mut both = event("b", None, None);
        both.world = true;
        both.server = Some("server.dcl.eth".into());
        both.place_id = Some("mirrored.dcl.eth".into());
        assert_eq!(destination_id(&both).as_deref(), Some("mirrored.dcl.eth"));
    }

    #[test]
    fn the_v1_record_flattens_the_event_and_adds_the_destination() {
        let mut e = event("e", None, None);
        e.place_id = Some("p".into());
        let v = serde_json::to_value(V1EventRecord {
            destination_id: destination_id(&e),
            destination: Some(serde_json::json!({ "id": "p", "kind": "place" })),
            event: e,
        })
        .unwrap();
        assert_eq!(v["id"], "e");
        assert_eq!(v["destination_id"], "p");
        assert_eq!(v["destination"]["kind"], "place");
    }
}
