use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::auth_chain::require_signer;
use crate::handlers::events::{is_admin_request, is_inside_world_limits};
use crate::http::response::{ApiError, ApiErrorBody, ApiOk};
use crate::schemas::EventRecord;
use crate::AppState;

const MAX_RECURRENT_PAST_ITERATIONS: i64 = 50_000;
const ADMIN_SIGNER: &str = "admin";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateEventBody {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_vertical: Option<String>,
    pub start_at: String,
    pub duration: i64,
    #[serde(default)]
    pub all_day: Option<bool>,
    #[serde(default)]
    pub recurrent: Option<bool>,
    #[serde(default)]
    pub recurrent_frequency: Option<String>,
    #[serde(default)]
    pub recurrent_setpos: Option<i64>,
    #[serde(default)]
    pub recurrent_monthday: Option<i64>,
    #[serde(default)]
    pub recurrent_weekday_mask: Option<i64>,
    #[serde(default)]
    pub recurrent_month_mask: Option<i64>,
    #[serde(default)]
    pub recurrent_interval: Option<i64>,
    #[serde(default)]
    pub recurrent_count: Option<i64>,
    #[serde(default)]
    pub recurrent_until: Option<String>,
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub schedules: Option<Vec<String>>,
    #[serde(default)]
    pub world: Option<bool>,
    #[serde(default)]
    pub community_id: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateEventBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_vertical: Option<String>,
    #[serde(default)]
    pub start_at: Option<String>,
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(default)]
    pub all_day: Option<bool>,
    #[serde(default)]
    pub recurrent: Option<bool>,
    #[serde(default)]
    pub recurrent_frequency: Option<String>,
    #[serde(default)]
    pub recurrent_setpos: Option<i64>,
    #[serde(default)]
    pub recurrent_monthday: Option<i64>,
    #[serde(default)]
    pub recurrent_weekday_mask: Option<i64>,
    #[serde(default)]
    pub recurrent_month_mask: Option<i64>,
    #[serde(default)]
    pub recurrent_interval: Option<i64>,
    #[serde(default)]
    pub recurrent_count: Option<i64>,
    #[serde(default)]
    pub recurrent_until: Option<String>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub schedules: Option<Vec<String>>,
    #[serde(default)]
    pub world: Option<bool>,
    #[serde(default)]
    pub community_id: Option<String>,
    #[serde(default)]
    pub approved: Option<bool>,
    #[serde(default)]
    pub rejected: Option<bool>,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    #[serde(default)]
    pub highlighted: Option<bool>,
    #[serde(default)]
    pub trending: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AdminPatchEventBody {
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
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
}

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
pub struct DeleteEventBody {
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn jump_in_url(world: bool, server: Option<&str>, x: i32, y: i32) -> String {
    match server {
        Some(s) if world && !s.is_empty() => {
            format!("https://decentraland.org/play/?realm={s}")
        }
        _ => format!("https://decentraland.org/play/?position={x}%2C{y}"),
    }
}

fn compute_next(
    start_at: DateTime<Utc>,
    duration_ms: i64,
    recurrent_dates: &[DateTime<Utc>],
) -> (DateTime<Utc>, DateTime<Utc>) {
    let span = Duration::milliseconds(duration_ms.max(0));
    let now = Utc::now();
    let next = recurrent_dates
        .iter()
        .copied()
        .filter(|d| *d + span > now)
        .min()
        .unwrap_or(start_at);
    (next, next + span)
}

fn to_dates(values: &[DateTime<Utc>]) -> Value {
    Value::Array(values.iter().map(|d| json!(d.to_rfc3339())).collect())
}

fn dedup_schedules(schedules: Option<Vec<String>>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    schedules
        .unwrap_or_default()
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

fn validate_create(body: &CreateEventBody) -> Result<DateTime<Utc>, ApiError> {
    if body.name.chars().count() > 150 {
        return Err(ApiError::bad_request("name must be at most 150 characters"));
    }
    if let Some(d) = &body.description {
        if d.chars().count() > 5000 {
            return Err(ApiError::bad_request("description too long"));
        }
    }
    if body.duration < 0 {
        return Err(ApiError::bad_request("duration must be positive"));
    }
    let start_at =
        parse_rfc3339(&body.start_at).ok_or_else(|| ApiError::bad_request("invalid start_at"))?;
    if !(-170..=170).contains(&body.x) || !(-170..=170).contains(&body.y) {
        return Err(ApiError::bad_request("coordinates out of range"));
    }
    let world = body.world.unwrap_or(false);
    if !world && !is_inside_world_limits(body.x, body.y) {
        return Err(ApiError::bad_request("coordinates outside world limits"));
    }
    Ok(start_at)
}

fn build_create_raw(body: &CreateEventBody, signer: &str, start_at: DateTime<Utc>) -> Value {
    let now = Utc::now();
    let world = body.world.unwrap_or(false);
    // Recurrence expansion is not implemented: a recurrent event still gets the
    // single start date. Both arms of the original branch produced exactly this,
    // so collapsing changes nothing but makes the gap legible.
    let recurrent_dates = vec![start_at];
    let (next_start, next_finish) = compute_next(start_at, body.duration, &recurrent_dates);
    let finish_at = start_at + Duration::milliseconds(body.duration.max(0));
    let url = body
        .url
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| jump_in_url(world, body.server.as_deref(), body.x, body.y));
    let schedules = dedup_schedules(body.schedules.clone());

    let core: Value = json!({
        "id": null,
        "name": body.name,
        "image": body.image,
        "image_vertical": body.image_vertical,
        "description": body.description,
        "start_at": start_at.to_rfc3339(),
        "finish_at": finish_at.to_rfc3339(),
        "next_start_at": next_start.to_rfc3339(),
        "next_finish_at": next_finish.to_rfc3339(),
        "duration": body.duration,
        "all_day": body.all_day.unwrap_or(false),
        "x": body.x,
        "y": body.y,
        "server": body.server,
        "url": url,
        "user": signer,
        "user_name": null,
        "estate_id": null,
        "estate_name": null,
        "scene_name": null,
    });

    let recurrence: Value = json!({
        "approved": false,
        "rejected": false,
        "highlighted": false,
        "trending": false,
        "world": world,
        "recurrent": body.recurrent.unwrap_or(false),
        "recurrent_frequency": body.recurrent_frequency,
        "recurrent_setpos": body.recurrent_setpos,
        "recurrent_monthday": body.recurrent_monthday,
        "recurrent_weekday_mask": body.recurrent_weekday_mask.unwrap_or(0),
        "recurrent_month_mask": body.recurrent_month_mask.unwrap_or(0),
        "recurrent_interval": body.recurrent_interval.unwrap_or(1),
        "recurrent_count": body.recurrent_count,
        "recurrent_until": body.recurrent_until,
        "recurrent_dates": to_dates(&recurrent_dates),
        "categories": body.categories.clone().unwrap_or_default(),
        "schedules": schedules,
        "contact": body.contact,
        "details": body.details,
    });

    let attendance: Value = json!({
        "total_attendees": 0,
        "latest_attendees": [],
        "coordinates": [body.x, body.y],
        "position": [body.x, body.y],
        "live": false,
        "attending": false,
        "place_id": null,
        "community_id": body.community_id,
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339(),
        "approved_by": null,
        "rejected_by": null,
        "rejection_reason": null,
        "deleted_by_user": false,
        "deleted_by_admin": false,
        "deleted_by": null,
        "deleted_at": null,
        "deleted_reason": null,
        "previous_place_id": null,
    });

    let mut merged = core.as_object().cloned().unwrap_or_default();
    if let Some(obj) = recurrence.as_object() {
        merged.extend(obj.clone());
    }
    if let Some(obj) = attendance.as_object() {
        merged.extend(obj.clone());
    }
    Value::Object(merged)
}

fn generate_id(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update(Utc::now().timestamp_nanos_opt().unwrap_or(0).to_be_bytes());
    let h = hex::encode(hasher.finalize());
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

#[utoipa::path(
    post,
    path = "/api/events",
    tag = "events",
    request_body = CreateEventBody,
    responses(
        (status = 200, body = ApiOk<EventRecord>),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiOk<EventRecord>>, ApiError> {
    let admin = is_admin_request(&headers, &state);
    let signer = if admin {
        ADMIN_SIGNER.to_string()
    } else {
        require_signer(&headers, "post", "/api/events")
            .await?
            .as_str()
            .to_lowercase()
    };
    let body: CreateEventBody = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("invalid event body: {e}")))?;
    let start_at = validate_create(&body)?;
    let id = generate_id(&body.name);
    let raw = build_create_raw(&body, &signer, start_at);
    let record = state.events.write_event(&id, &raw, &signer).await?;
    Ok(Json(ApiOk::new(record)))
}

fn set_opt_str(map: &mut Map<String, Value>, key: &str, val: &Option<String>) {
    if let Some(v) = val {
        map.insert(key.into(), json!(v));
    }
}

fn set_opt_i64(map: &mut Map<String, Value>, key: &str, val: Option<i64>) {
    if let Some(v) = val {
        map.insert(key.into(), json!(v));
    }
}

fn set_opt_bool(map: &mut Map<String, Value>, key: &str, val: Option<bool>) {
    if let Some(v) = val {
        map.insert(key.into(), json!(v));
    }
}

fn recurrence_touched(body: &UpdateEventBody) -> bool {
    body.recurrent.is_some()
        || body.recurrent_frequency.is_some()
        || body.recurrent_setpos.is_some()
        || body.recurrent_monthday.is_some()
        || body.recurrent_weekday_mask.is_some()
        || body.recurrent_month_mask.is_some()
        || body.recurrent_interval.is_some()
        || body.recurrent_count.is_some()
        || body.recurrent_until.is_some()
        || body.start_at.is_some()
        || body.duration.is_some()
}

fn content_touched(body: &UpdateEventBody) -> bool {
    body.name.is_some()
        || body.description.is_some()
        || body.image.is_some()
        || body.image_vertical.is_some()
        || body.start_at.is_some()
        || body.duration.is_some()
        || body.x.is_some()
        || body.y.is_some()
        || body.server.is_some()
        || body.url.is_some()
        || body.categories.is_some()
        || body.schedules.is_some()
        || body.world.is_some()
        || body.community_id.is_some()
        || recurrence_touched(body)
}

fn apply_owner_edit(raw: &mut Map<String, Value>, body: &UpdateEventBody) -> Result<(), ApiError> {
    if let Some(name) = &body.name {
        if name.chars().count() > 150 {
            return Err(ApiError::bad_request("name must be at most 150 characters"));
        }
        raw.insert("name".into(), json!(name));
    }
    set_opt_str(raw, "description", &body.description);
    set_opt_str(raw, "image", &body.image);
    set_opt_str(raw, "image_vertical", &body.image_vertical);
    set_opt_str(raw, "server", &body.server);
    set_opt_str(raw, "contact", &body.contact);
    set_opt_str(raw, "details", &body.details);
    set_opt_str(raw, "recurrent_frequency", &body.recurrent_frequency);
    set_opt_str(raw, "recurrent_until", &body.recurrent_until);
    set_opt_str(raw, "community_id", &body.community_id);
    set_opt_bool(raw, "all_day", body.all_day);
    set_opt_bool(raw, "recurrent", body.recurrent);
    set_opt_bool(raw, "world", body.world);
    set_opt_i64(raw, "recurrent_setpos", body.recurrent_setpos);
    set_opt_i64(raw, "recurrent_monthday", body.recurrent_monthday);
    set_opt_i64(raw, "recurrent_weekday_mask", body.recurrent_weekday_mask);
    set_opt_i64(raw, "recurrent_month_mask", body.recurrent_month_mask);
    set_opt_i64(raw, "recurrent_interval", body.recurrent_interval);
    set_opt_i64(raw, "recurrent_count", body.recurrent_count);
    set_opt_i64(raw, "duration", body.duration);
    if let Some(url) = &body.url {
        raw.insert("url".into(), json!(url));
    }
    if let Some(cats) = &body.categories {
        raw.insert("categories".into(), json!(cats));
    }
    if let Some(scheds) = &body.schedules {
        raw.insert(
            "schedules".into(),
            json!(dedup_schedules(Some(scheds.clone()))),
        );
    }
    if let Some(x) = body.x {
        if !(-170..=170).contains(&x) {
            return Err(ApiError::bad_request("coordinates out of range"));
        }
        raw.insert("x".into(), json!(x));
        raw.insert(
            "coordinates".into(),
            json!([
                x,
                body.y
                    .unwrap_or_else(|| raw.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
            ]),
        );
    }
    if let Some(y) = body.y {
        if !(-170..=170).contains(&y) {
            return Err(ApiError::bad_request("coordinates out of range"));
        }
        raw.insert("y".into(), json!(y));
    }
    if let Some(start_at) = &body.start_at {
        let parsed =
            parse_rfc3339(start_at).ok_or_else(|| ApiError::bad_request("invalid start_at"))?;
        raw.insert("start_at".into(), json!(parsed.to_rfc3339()));
    }

    if recurrence_touched(body) {
        recompute_occurrences(raw)?;
    }

    let x = raw.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let y = raw.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    raw.insert("position".into(), json!([x, y]));
    raw.insert("coordinates".into(), json!([x, y]));

    Ok(())
}

fn recompute_occurrences(raw: &mut Map<String, Value>) -> Result<(), ApiError> {
    let start_at = raw
        .get("start_at")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339);
    let duration = raw.get("duration").and_then(|v| v.as_i64()).unwrap_or(0);
    if let Some(count) = raw.get("recurrent_count").and_then(|v| v.as_i64()) {
        if count > MAX_RECURRENT_PAST_ITERATIONS {
            return Err(ApiError::bad_request(
                "recurrence would exceed the maximum number of occurrences",
            ));
        }
    }
    if let Some(start) = start_at {
        let dates = vec![start];
        let (next_start, next_finish) = compute_next(start, duration, &dates);
        raw.insert("recurrent_dates".into(), to_dates(&dates));
        raw.insert("next_start_at".into(), json!(next_start.to_rfc3339()));
        raw.insert("next_finish_at".into(), json!(next_finish.to_rfc3339()));
        raw.insert(
            "finish_at".into(),
            json!((start + Duration::milliseconds(duration.max(0))).to_rfc3339()),
        );
    }
    Ok(())
}

fn apply_admin_state(
    raw: &mut Map<String, Value>,
    body: &AdminPatchEventBody,
) -> Result<(), ApiError> {
    if let Some(reason) = &body.reason {
        if reason.chars().count() > 500 {
            return Err(ApiError::bad_request("reason too long"));
        }
    }
    if let Some(actor) = &body.actor {
        if actor.chars().count() > 42 {
            return Err(ApiError::bad_request("actor too long"));
        }
    }
    let actor = body.actor.clone();
    let mut handled = false;
    match body.action.as_deref() {
        Some("approve") => {
            set_approved(raw, actor.clone());
            handled = true;
        }
        Some("reject") | Some("archive") => {
            set_rejected(raw, actor.clone(), body.reason.clone());
            handled = true;
        }
        Some("feature") => {
            raw.insert("highlighted".into(), json!(true));
            handled = true;
        }
        Some("unfeature") => {
            raw.insert("highlighted".into(), json!(false));
            handled = true;
        }
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "unknown action \"{other}\" (expected approve|reject|feature|unfeature|archive)"
            )));
        }
        None => {}
    }

    if let Some(v) = body.approved {
        if v {
            set_approved(raw, actor.clone());
        } else {
            raw.insert("approved".into(), json!(false));
        }
        handled = true;
    }
    if let Some(v) = body.rejected {
        if v {
            set_rejected(raw, actor.clone(), body.reason.clone());
        } else {
            raw.insert("rejected".into(), json!(false));
            raw.insert("rejected_by".into(), Value::Null);
        }
        handled = true;
    }
    if let Some(v) = body.highlighted {
        raw.insert("highlighted".into(), json!(v));
        handled = true;
    }
    if let Some(v) = body.trending {
        raw.insert("trending".into(), json!(v));
        handled = true;
    }
    if let Some(name) = &body.name {
        raw.insert("name".into(), json!(name));
        handled = true;
    }
    if let Some(description) = &body.description {
        raw.insert("description".into(), json!(description));
        handled = true;
    }

    if !handled {
        return Err(ApiError::bad_request("no moderation fields provided"));
    }
    Ok(())
}

fn set_approved(raw: &mut Map<String, Value>, actor: Option<String>) {
    raw.insert("approved".into(), json!(true));
    raw.insert(
        "approved_by".into(),
        actor.map(Value::from).unwrap_or(Value::Null),
    );
    raw.insert("rejected".into(), json!(false));
    raw.insert("rejected_by".into(), Value::Null);
    raw.insert("rejection_reason".into(), Value::Null);
}

fn set_rejected(raw: &mut Map<String, Value>, actor: Option<String>, reason: Option<String>) {
    raw.insert("approved".into(), json!(false));
    raw.insert("approved_by".into(), Value::Null);
    raw.insert("rejected".into(), json!(true));
    raw.insert(
        "rejected_by".into(),
        actor.map(Value::from).unwrap_or(Value::Null),
    );
    raw.insert(
        "rejection_reason".into(),
        reason.map(Value::from).unwrap_or(Value::Null),
    );
}

async fn load_raw_map(
    state: &AppState,
    id: &str,
) -> Result<Option<(Map<String, Value>, String)>, ApiError> {
    let Some((raw, user_creator)) = state.events.get_raw(id).await? else {
        return Ok(None);
    };
    let owner = raw
        .get("user")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or(user_creator)
        .unwrap_or_default()
        .to_lowercase();
    let map = match raw {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    Ok(Some((map, owner)))
}

fn is_soft_deleted(raw: &Map<String, Value>) -> bool {
    raw.get("deleted_by_user")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || raw
            .get("deleted_by_admin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

#[utoipa::path(
    patch,
    path = "/api/events/{event_id}",
    tag = "events",
    params(("event_id" = String, Path)),
    request_body = UpdateEventBody,
    responses(
        (status = 200, body = ApiOk<EventRecord>),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn patch_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
    body: Bytes,
) -> Result<Json<ApiOk<EventRecord>>, ApiError> {
    let path = format!("/api/events/{event_id}");
    if is_admin_request(&headers, &state) {
        let body: AdminPatchEventBody = serde_json::from_slice(&body)
            .map_err(|e| ApiError::bad_request(format!("invalid patch body: {e}")))?;
        let Some((mut raw, _owner)) = load_raw_map(&state, &event_id).await? else {
            return Err(ApiError::not_found(format!(
                "Not found event \"{event_id}\""
            )));
        };
        apply_admin_state(&mut raw, &body)?;
        raw.insert("updated_at".into(), json!(Utc::now().to_rfc3339()));
        let signer = body.actor.clone().unwrap_or_else(|| ADMIN_SIGNER.into());
        let record = state
            .events
            .write_event(&event_id, &Value::Object(raw), &signer)
            .await?;
        return Ok(Json(ApiOk::new(record)));
    }

    let signer = require_signer(&headers, "patch", &path)
        .await?
        .as_str()
        .to_lowercase();
    let body: UpdateEventBody = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("invalid patch body: {e}")))?;

    let Some((mut raw, owner)) = load_raw_map(&state, &event_id).await? else {
        return Err(ApiError::not_found(format!(
            "Not found event \"{event_id}\""
        )));
    };
    if is_soft_deleted(&raw) {
        return Err(ApiError::not_found(format!(
            "Not found event \"{event_id}\""
        )));
    }
    if owner != signer {
        return Err(ApiError::forbidden(
            "You don't have permission to edit this event",
        ));
    }

    let was_approved = raw
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    apply_owner_edit(&mut raw, &body)?;
    if was_approved && content_touched(&body) {
        raw.insert("approved".into(), json!(false));
    }
    raw.insert("updated_at".into(), json!(Utc::now().to_rfc3339()));

    let record = state
        .events
        .write_event(&event_id, &Value::Object(raw), &signer)
        .await?;
    Ok(Json(ApiOk::new(record)))
}

#[utoipa::path(
    delete,
    path = "/api/events/{event_id}",
    tag = "events",
    params(("event_id" = String, Path)),
    request_body = DeleteEventBody,
    responses(
        (status = 200, body = ApiOk<EventRecord>),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 500, body = ApiErrorBody)
    )
)]
pub async fn delete_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
    body: Bytes,
) -> Result<Json<ApiOk<EventRecord>>, ApiError> {
    let admin = is_admin_request(&headers, &state);
    let path = format!("/api/events/{event_id}");
    let signer = if admin {
        None
    } else {
        Some(
            require_signer(&headers, "delete", &path)
                .await?
                .as_str()
                .to_lowercase(),
        )
    };

    let Some((mut raw, owner)) = load_raw_map(&state, &event_id).await? else {
        return Err(ApiError::not_found(format!(
            "Not found event \"{event_id}\""
        )));
    };

    if is_soft_deleted(&raw) {
        let record = state
            .events
            .get(&event_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("Not found event \"{event_id}\"")))?;
        return Ok(Json(ApiOk::new(record)));
    }

    let now = Utc::now().to_rfc3339();
    let provenance = if admin {
        let body: DeleteEventBody = if body.is_empty() {
            DeleteEventBody::default()
        } else {
            serde_json::from_slice(&body)
                .map_err(|e| ApiError::bad_request(format!("invalid delete body: {e}")))?
        };
        raw.insert("deleted_by_admin".into(), json!(true));
        raw.insert(
            "deleted_by".into(),
            body.actor.clone().map(Value::from).unwrap_or(Value::Null),
        );
        raw.insert(
            "deleted_reason".into(),
            body.reason.clone().map(Value::from).unwrap_or(Value::Null),
        );
        body.actor.unwrap_or_else(|| ADMIN_SIGNER.into())
    } else {
        let signer = signer.unwrap();
        if owner != signer {
            return Err(ApiError::forbidden(
                "You don't have permission to delete this event",
            ));
        }
        raw.insert("deleted_by_user".into(), json!(true));
        raw.insert("deleted_by".into(), json!(signer));
        signer
    };
    raw.insert("deleted_at".into(), json!(now));
    raw.insert("updated_at".into(), json!(Utc::now().to_rfc3339()));

    let record = state
        .events
        .write_event(&event_id, &Value::Object(raw), &provenance)
        .await?;
    Ok(Json(ApiOk::new(record)))
}
