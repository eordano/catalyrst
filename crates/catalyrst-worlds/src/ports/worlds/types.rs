use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::access::AccessSetting;
use crate::settings_policy::{
    js_truthy, storable_skybox_time, text_len, DESCRIPTION_MAX_LENGTH, DESCRIPTION_MIN_LENGTH,
    MAX_CATEGORIES, TITLE_MAX_LENGTH, TITLE_MIN_LENGTH, VALID_RATINGS,
};

#[derive(Debug, Clone)]
pub struct WorldRecord {
    pub name: String,
    pub owner: Option<String>,
    pub access: AccessSetting,
    pub blocked_since: Option<DateTime<Utc>>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub single_player: bool,
    pub realm_name_override: Option<String>,
    pub preview_wearable_urns: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct WorldScene {
    pub entity_id: String,
    pub entity: Value,
    pub parcels: Vec<String>,
    /// The zero address marks a scene this node mirrored rather than one an
    /// owner deployed here; the realm-name policy turns on that distinction.
    pub deployer: String,
}

/// Which already-deployed scenes a deploy is authorized to replace when it lands.
///
/// A world-name owner may replace every overlapping scene; a parcel-scoped deployer may
/// replace only the exact scene identities whose full footprints its permission covered,
/// so it can't silently remove a scene reaching into parcels it was never granted.
#[derive(Debug, Clone)]
pub enum SceneReplacement {
    UnrestrictedOwner,
    Scoped(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct WorldSceneRow {
    pub world_name: String,
    pub entity_id: String,
    pub deployment_auth_chain: Value,
    pub entity: Value,
    pub deployer: String,
    pub parcels: Vec<String>,
    pub size: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WorldsCount {
    pub ens: i64,
    pub dcl: i64,
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
    pub single_player: bool,
    pub show_in_places: bool,
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
    pub access_type: Option<String>,
    pub realm_name_override: Option<String>,
    pub preview_wearable_urns: Option<Vec<String>>,
    pub settings_version: i64,
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
    pub realm_name_override: Option<String>,
    pub realm_name_override_provided: bool,
    pub preview_wearable_urns: Option<Vec<String>>,
    pub preview_wearable_urns_provided: bool,
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

// Scene metadata is deployer-controlled and unconstrained, so every deploy-derived
// value is held to the same allow-list PUT /settings enforces; a value the policy
// rejects resolves to None ("not expressed") rather than being stored or corrupted,
// and None lets the deploy path preserve whatever the owner already configured.
pub(super) fn scene_settings_from_entity(entity: &Value) -> DerivedSceneSettings {
    let meta = entity.get("metadata");
    let display = meta.and_then(|m| m.get("display"));
    let wc = meta.and_then(|m| m.get("worldConfiguration"));
    let scene = meta.and_then(|m| m.get("scene"));

    let title = display
        .and_then(|d| d.get("title"))
        .and_then(|v| v.as_str())
        .filter(|t| (TITLE_MIN_LENGTH..=TITLE_MAX_LENGTH).contains(&text_len(t)))
        .map(str::to_string);
    let description = display
        .and_then(|d| d.get("description"))
        .and_then(|v| v.as_str())
        .filter(|d| (DESCRIPTION_MIN_LENGTH..=DESCRIPTION_MAX_LENGTH).contains(&text_len(d)))
        .map(str::to_string);
    let content_rating = meta
        .and_then(|m| m.get("rating"))
        .and_then(|v| v.as_str())
        .filter(|r| VALID_RATINGS.contains(r))
        .map(str::to_string);
    let skybox_time = wc
        .and_then(|c| c.get("skyboxConfig"))
        .and_then(|s| s.get("fixedTime"))
        .and_then(|v| v.as_f64())
        .and_then(storable_skybox_time);
    let categories = meta
        .and_then(|m| m.get("tags"))
        .and_then(|t| t.as_array())
        .filter(|arr| !arr.is_empty() && arr.len() <= MAX_CATEGORIES)
        .and_then(|arr| {
            arr.iter()
                .map(|t| t.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        });
    // None when the scene says nothing, so "not declared" stays distinguishable
    // from "declared false" and a redeploy cannot silently revert owner settings.
    let single_player = wc
        .and_then(|c| c.get("fixedAdapter"))
        .map(|v| v.as_str() == Some("offline:offline"));
    let show_in_places = wc
        .and_then(|c| c.get("placesConfig"))
        .and_then(|p| p.get("optOut"))
        .map(|v| !js_truthy(v));
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
    catalyrst_types::pointer::canonicalize_pointer(s)
}

pub(super) fn canonicalize_parcels(parcels: &[String]) -> Vec<String> {
    parcels.iter().map(|p| canonicalize_parcel(p)).collect()
}

/// The canonical parcel used as a scene's downstream identity (comms room / ban keys).
///
/// The declared `metadata.scene.base` is trusted only when it belongs to the scene's
/// canonicalized stored parcels; corrupt or attacker-controlled metadata that names a base
/// outside the footprint falls back to the first canonical parcel rather than resolving to
/// some other scene's identity. Returns `None` only for a row with no usable parcels.
pub(super) fn effective_base_parcel(entity: &Value, parcels: &[String]) -> Option<String> {
    let canonical_parcels = canonicalize_parcels(parcels);
    let declared = entity
        .get("metadata")
        .and_then(|m| m.get("scene"))
        .and_then(|s| s.get("base"))
        .and_then(|b| b.as_str())
        .filter(|s| !s.is_empty());
    if let Some(base) = declared {
        let canonical_base = canonicalize_parcel(base);
        if canonical_parcels.contains(&canonical_base) {
            return Some(canonical_base);
        }
    }
    canonical_parcels.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entity_with_wc(wc: Value) -> Value {
        json!({ "metadata": { "worldConfiguration": wc } })
    }

    fn entity_with_base(base: Value) -> Value {
        json!({ "metadata": { "scene": { "base": base } } })
    }

    #[test]
    fn effective_base_trusts_declared_base_only_when_a_member() {
        let parcels = vec!["0,0".to_string(), "0,1".to_string()];

        // Declared base inside the footprint is used (canonicalized).
        assert_eq!(
            effective_base_parcel(&entity_with_base(json!("0,1")), &parcels),
            Some("0,1".to_string())
        );
        assert_eq!(
            effective_base_parcel(&entity_with_base(json!(" 00 , 01 ")), &parcels),
            Some("0,1".to_string())
        );

        // Declared base outside the footprint is rejected: fall back to first canonical parcel,
        // never resolving to some other scene's identity.
        assert_eq!(
            effective_base_parcel(&entity_with_base(json!("9,9")), &parcels),
            Some("0,0".to_string())
        );
        // Absent / empty base also falls back to the first canonical parcel.
        assert_eq!(
            effective_base_parcel(&json!({ "metadata": { "scene": {} } }), &parcels),
            Some("0,0".to_string())
        );
        assert_eq!(
            effective_base_parcel(&entity_with_base(json!("")), &parcels),
            Some("0,0".to_string())
        );
        // No parcels at all: no usable identity.
        assert_eq!(
            effective_base_parcel(&entity_with_base(json!("0,0")), &[]),
            None
        );
    }

    #[test]
    fn opt_out_uses_js_truthiness() {
        let opt_out = |v: Value| {
            scene_settings_from_entity(&entity_with_wc(json!({ "placesConfig": { "optOut": v } })))
                .show_in_places
        };
        assert_eq!(opt_out(json!(true)), Some(false));
        assert_eq!(opt_out(json!(1)), Some(false));
        assert_eq!(opt_out(json!("false")), Some(false));
        assert_eq!(opt_out(json!({})), Some(false));
        assert_eq!(opt_out(json!(false)), Some(true));
        assert_eq!(opt_out(json!(0)), Some(true));
        assert_eq!(opt_out(json!(null)), Some(true));
        let undeclared = scene_settings_from_entity(&entity_with_wc(json!({ "placesConfig": {} })));
        assert_eq!(undeclared.show_in_places, None);
    }

    #[test]
    fn text_bounds_are_utf16_code_units() {
        let entity = json!({ "metadata": { "display": {
            "title": "\u{65E5}\u{672C}",
            "description": "\u{30C7}".repeat(400),
        } } });
        let s = scene_settings_from_entity(&entity);
        assert_eq!(s.title, None, "2 UTF-16 units is under the 3-unit minimum");
        assert_eq!(
            s.description.as_deref().map(text_len),
            Some(400),
            "400 UTF-16 units is within the 1000-unit maximum despite 1200 bytes"
        );
    }

    #[test]
    fn skybox_time_shares_the_settings_policy_coercion() {
        let skybox = |v: Value| {
            scene_settings_from_entity(&entity_with_wc(
                json!({ "skyboxConfig": { "fixedTime": v } }),
            ))
            .skybox_time
        };
        assert_eq!(skybox(json!(36000)), Some(36000));
        assert_eq!(skybox(json!(36000.0)), Some(36000));
        assert_eq!(skybox(json!(1.5)), None);
        assert_eq!(skybox(json!(99999999999i64)), None);
    }
}
