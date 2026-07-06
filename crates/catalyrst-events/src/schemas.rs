use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventRecord {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub connected_addresses: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventCategoryRecord {
    pub name: String,
    pub active: bool,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub i18n: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "events/"))]
pub struct EventAttendeeRecord {
    pub event_id: String,
    pub user: String,
    pub user_name: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
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
