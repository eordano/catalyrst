use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventRecord {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    #[schema(value_type = Option<String>)]
    pub image_vertical: Option<Value>,
    pub description: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub start_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub finish_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub next_start_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub next_finish_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub duration: Option<i64>,
    pub all_day: bool,
    pub x: i32,
    pub y: i32,
    pub server: Option<String>,
    pub url: Option<String>,
    pub user: Option<String>,
    pub user_name: Option<String>,
    pub estate_id: Option<String>,
    pub estate_name: Option<String>,
    pub scene_name: Option<String>,
    pub approved: bool,
    pub rejected: bool,
    pub highlighted: bool,
    pub trending: bool,
    pub world: bool,
    pub recurrent: bool,
    pub recurrent_frequency: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub recurrent_weekday_mask: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub recurrent_month_mask: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub recurrent_interval: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub recurrent_setpos: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub recurrent_monthday: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub recurrent_count: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub recurrent_until: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "Array<string>"))]
    pub recurrent_dates: Vec<DateTime<Utc>>,
    pub categories: Vec<String>,
    pub schedules: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_attendees: i64,
    pub latest_attendees: Vec<String>,
    pub coordinates: [i32; 2],
    pub position: [i32; 2],
    pub live: bool,
    pub attending: bool,
    pub place_id: Option<String>,
    pub community_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub created_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub updated_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub rejected_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub deleted_by_user: bool,
    pub deleted_by_admin: bool,
    pub deleted_by: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_reason: Option<String>,
    pub previous_place_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub connected_addresses: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventCategoryRecord {
    pub name: String,
    pub active: bool,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    #[schema(value_type = Object)]
    pub i18n: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventAttendeeRecord {
    pub event_id: String,
    pub user: String,
    pub user_name: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct ScheduleUpsertMessage {
    #[serde(default)]
    pub schedule_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub background: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub active_since: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub active_until: i64,
    pub active: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub signed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct ScheduleUpsertEnvelope {
    #[schema(value_type = Object)]
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub domain: Value,
    pub message: ScheduleUpsertMessage,
    #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
    pub nonce: Vec<u8>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub signed_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct ScheduleRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub theme: Option<String>,
    pub background: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub active_since: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub active_until: Option<DateTime<Utc>>,
    pub active: bool,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub created_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub updated_at: Option<DateTime<Utc>>,
}
