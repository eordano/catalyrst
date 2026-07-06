use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::access::AccessSetting;

#[derive(Debug, Clone)]
pub struct WorldRecord {
    pub name: String,
    pub owner: Option<String>,
    pub access: AccessSetting,
    pub blocked_since: Option<DateTime<Utc>>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub single_player: bool,
}

#[derive(Debug, Clone)]
pub struct WorldScene {
    pub entity_id: String,
    pub entity: Value,
    pub parcels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorldAdminRow {
    pub name: String,
    pub owner: Option<String>,
    pub access_type: String,
    pub blocked_since: Option<DateTime<Utc>>,
    pub spawn_coordinates: Option<String>,
    pub scene_count: i64,
}

#[derive(Debug, Clone)]
pub struct BlockedRow {
    pub wallet: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AccessLogRow {
    pub id: i64,
    pub world_name: String,
    pub address: String,
    pub action: String,
    pub room: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldsOrderBy {
    Name,
    LastDeployedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Default)]
pub struct WorldsListFilters {
    pub authorized_deployer: Option<String>,
    pub search: Option<String>,
    pub has_deployed_scenes: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct WorldsListOptions {
    pub limit: i64,
    pub offset: i64,
    pub order_by: WorldsOrderBy,
    pub order_direction: OrderDirection,
}

#[derive(Debug, Clone)]
pub struct WorldInfoRow {
    pub name: String,
    pub owner: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub categories: Option<Vec<String>>,
    pub single_player: Option<bool>,
    pub show_in_places: Option<bool>,
    pub thumbnail_hash: Option<String>,
    pub last_deployed_at: Option<DateTime<Utc>>,
    pub min_x: Option<i32>,
    pub max_x: Option<i32>,
    pub min_y: Option<i32>,
    pub max_y: Option<i32>,
    pub blocked_since: Option<DateTime<Utc>>,
    pub deployed_scenes: i64,
}

#[derive(Debug, Clone, Default)]
pub struct WorldSettingsRow {
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub categories: Option<Vec<String>>,
    pub single_player: Option<bool>,
    pub show_in_places: Option<bool>,
    pub thumbnail_hash: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorldSettingsUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub skybox_time_provided: bool,
    pub categories: Option<Vec<String>>,
    pub categories_provided: bool,
    pub single_player: Option<bool>,
    pub show_in_places: Option<bool>,
    pub thumbnail_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorldManifest {
    pub parcels: Vec<String>,
    pub spawn_coordinates: Option<String>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct PermissionRecordFull {
    pub id: i32,
    pub permission_type: String,
    pub address: String,
    pub is_world_wide: bool,
    pub parcel_count: i64,
}

pub(super) struct DerivedSceneSettings {
    pub(super) spawn_coordinates: Option<String>,
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    pub(super) content_rating: Option<String>,
    pub(super) skybox_time: Option<i32>,
    pub(super) categories: Option<Vec<String>>,
    pub(super) single_player: Option<bool>,
    pub(super) show_in_places: Option<bool>,
    pub(super) thumbnail_hash: Option<String>,
}

pub(super) fn scene_settings_from_entity(entity: &Value) -> DerivedSceneSettings {
    let meta = entity.get("metadata");
    let display = meta.and_then(|m| m.get("display"));
    let wc = meta.and_then(|m| m.get("worldConfiguration"));
    let scene = meta.and_then(|m| m.get("scene"));

    let title = display
        .and_then(|d| d.get("title"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let description = display
        .and_then(|d| d.get("description"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let content_rating = meta
        .and_then(|m| m.get("rating"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let skybox_time = wc
        .and_then(|c| c.get("skyboxConfig"))
        .and_then(|s| s.get("fixedTime"))
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);
    let categories = meta
        .and_then(|m| m.get("tags"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty());
    let single_player = Some(
        wc.and_then(|c| c.get("fixedAdapter"))
            .and_then(|v| v.as_str())
            == Some("offline:offline"),
    );
    let opt_out = wc
        .and_then(|c| c.get("placesConfig"))
        .and_then(|p| p.get("optOut"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let show_in_places = Some(!opt_out);
    let spawn_coordinates = scene
        .and_then(|s| s.get("base"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            scene
                .and_then(|s| s.get("parcels"))
                .and_then(|p| p.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let thumbnail_hash = display
        .and_then(|d| d.get("navmapThumbnail"))
        .and_then(|v| v.as_str())
        .and_then(|file| {
            entity
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|c| c.get("file").and_then(|f| f.as_str()) == Some(file))
                        .and_then(|c| c.get("hash").and_then(|h| h.as_str()))
                        .map(str::to_string)
                })
        });

    DerivedSceneSettings {
        spawn_coordinates,
        title,
        description,
        content_rating,
        skybox_time,
        categories,
        single_player,
        show_in_places,
        thumbnail_hash,
    }
}

pub fn canonicalize_parcel(s: &str) -> String {
    let parse = |part: &str| -> Option<i64> {
        let t = part.trim();
        let digits = t.strip_prefix('-').unwrap_or(t);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        t.parse::<i64>().ok()
    };
    match s.split_once(',') {
        Some((a, b)) => match (parse(a), parse(b)) {
            (Some(x), Some(y)) => format!("{x},{y}"),
            _ => s.to_string(),
        },
        None => s.to_string(),
    }
}

pub(super) fn canonicalize_parcels(parcels: &[String]) -> Vec<String> {
    parcels.iter().map(|p| canonicalize_parcel(p)).collect()
}
