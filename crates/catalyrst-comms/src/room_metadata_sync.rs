use serde_json::Value;

use crate::handlers::scene_adapter::{fetch_world_scene_id, meta_str, realm_name_from_metadata};
use crate::http::{service_unavailable, ApiError};
use crate::livekit::{
    is_world_realm_name, scene_room_name, world_room_name, world_scene_room_name,
    BANNED_ADDRESSES_FIELD, SCENE_ADMINS_FIELD,
};
use crate::ports::extra_addresses::{load_place_info, try_load_place_info, PlaceInfo};
use crate::AppState;

/// The request context upstream derives a room name from
/// (`livekit.getRoomName(realmName, { isWorld, sceneId })`), carried alongside
/// the `place_id` our endpoints are keyed on so a request whose signed-fetch
/// metadata names no realm still reaches the world rooms it used to.
pub struct RoomContext {
    pub place_id: String,
    pub realm_name: Option<String>,
    pub scene_id: Option<String>,
    pub parcel: Option<String>,
}

impl RoomContext {
    pub fn from_metadata(metadata: &Value, place_id: &str) -> Self {
        Self {
            place_id: place_id.to_string(),
            realm_name: realm_name_from_metadata(metadata),
            scene_id: meta_str(metadata, "sceneId"),
            parcel: meta_str(metadata, "parcel"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RoomPlan {
    Skip,
    Scene(String),
    World {
        world: String,
        scene_id: Option<String>,
    },
    FromPlace,
}

fn plan(ctx: &RoomContext) -> RoomPlan {
    let Some(realm) = ctx.realm_name.as_deref() else {
        return RoomPlan::FromPlace;
    };
    if is_world_realm_name(realm) {
        return RoomPlan::World {
            world: realm.to_string(),
            scene_id: ctx.scene_id.clone(),
        };
    }
    match ctx.scene_id.as_deref() {
        Some(scene_id) => RoomPlan::Scene(scene_room_name(realm, scene_id)),
        None => RoomPlan::Skip,
    }
}

const PLACE_MISMATCH_MSG: &str =
    "place_id does not match the realm and parcel this request is signed for";

pub const PLACE_LOOKUP_UNAVAILABLE_MSG: &str =
    "the place catalog is unavailable, so this request cannot be bound to its room";

/// The world branch keys on the crate's `.eth`-suffix reading of the realm name,
/// where upstream decides `isWorld` from the signed realm's hostname; a deployment
/// that serves worlds under a non-`.eth` realm widens `is_world_realm_name` (or
/// reads the hostname the way upstream does) rather than special-casing this gate.
fn context_matches_place(ctx: &RoomContext, place: &PlaceInfo) -> bool {
    match ctx.realm_name.as_deref() {
        None => true,
        Some(realm) if is_world_realm_name(realm) => {
            place.world
                && place
                    .world_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(realm))
        }
        Some(_) => {
            !place.world
                && ctx.parcel.as_deref().is_some_and(|parcel| {
                    place.positions.iter().any(|p| p == parcel)
                        || place.base_position.as_deref() == Some(parcel)
                })
        }
    }
}

/// Upstream reads the place out of the same signed metadata it names the room
/// from (`getWorldScenePlace(realmName, parcel)` / `getPlaceByParcel(parcel)` in
/// scene-bans.ts); our endpoints take `place_id` from the request instead, so the
/// two have to be tied back together here -- without this a caller authorized on
/// one place could aim the LiveKit side effects at another scene's room.
async fn ensure_place_matches_context(state: &AppState, ctx: &RoomContext) -> Result<(), ApiError> {
    if state.places_pool.is_none() {
        return Ok(());
    }
    let place = try_load_place_info(state, &ctx.place_id)
        .await
        .map_err(|error| {
            tracing::error!(
                %error,
                place_id = %ctx.place_id,
                "places lookup failed while binding a mutation to its room"
            );
            service_unavailable(PLACE_LOOKUP_UNAVAILABLE_MSG)
        })?;
    match place {
        Some(place) if context_matches_place(ctx, &place) => Ok(()),
        _ => Err(ApiError::bad_request(PLACE_MISMATCH_MSG)),
    }
}

async fn world_rooms(state: &AppState, world: &str, scene_id: Option<&str>) -> Vec<String> {
    let mut rooms = vec![world_room_name(world)];
    let resolved = match scene_id {
        Some(id) if !is_world_realm_name(id) => Some(id.to_string()),
        _ => fetch_world_scene_id(state, world).await,
    };
    if let Some(id) = resolved {
        rooms.push(world_scene_room_name(world, &id));
    }
    rooms
}

/// The LiveKit rooms a ban/admin mutation must reflect into, resolved before the
/// database write so a failure to name the room cannot leave the two out of sync
/// (upstream computes `roomName` up front for the same reason).
pub async fn resolve_rooms(state: &AppState, ctx: &RoomContext) -> Result<Vec<String>, ApiError> {
    if !state.livekit_configured {
        return Ok(Vec::new());
    }
    match plan(ctx) {
        RoomPlan::Skip => Ok(Vec::new()),
        RoomPlan::Scene(room) => {
            ensure_place_matches_context(state, ctx).await?;
            Ok(vec![room])
        }
        RoomPlan::World { world, scene_id } => {
            ensure_place_matches_context(state, ctx).await?;
            Ok(world_rooms(state, &world, scene_id.as_deref()).await)
        }
        RoomPlan::FromPlace => Ok(match load_place_info(state, &ctx.place_id).await {
            Some(place) if place.world => match place.world_name.as_deref() {
                Some(world) => world_rooms(state, world, None).await,
                None => Vec::new(),
            },
            _ => Vec::new(),
        }),
    }
}

pub async fn add_ban(state: &AppState, rooms: &[String], address: &str) {
    mutate(state, rooms, BANNED_ADDRESSES_FIELD, address, Op::Append).await;
}

pub async fn remove_ban(state: &AppState, rooms: &[String], address: &str) {
    mutate(state, rooms, BANNED_ADDRESSES_FIELD, address, Op::Remove).await;
}

pub async fn add_admin(state: &AppState, rooms: &[String], address: &str) {
    mutate(state, rooms, SCENE_ADMINS_FIELD, address, Op::Append).await;
}

pub async fn remove_admin(state: &AppState, rooms: &[String], address: &str) {
    mutate(state, rooms, SCENE_ADMINS_FIELD, address, Op::Remove).await;
}

pub async fn kick(state: &AppState, rooms: &[String], address: &str) {
    if !state.livekit_configured || rooms.is_empty() {
        return;
    }
    let client = state.room_service();
    let addr = address.to_lowercase();
    for room in rooms {
        if let Err(error) = client.remove_participant(room, &addr).await {
            tracing::warn!(
                %error,
                room = %room,
                address = %addr,
                "failed to kick banned participant (best-effort)"
            );
        }
    }
}

#[derive(Clone, Copy)]
enum Op {
    Append,
    Remove,
}

async fn mutate(state: &AppState, rooms: &[String], field: &str, address: &str, op: Op) {
    if !state.livekit_configured || rooms.is_empty() {
        return;
    }
    let client = state.room_service();
    let addr = address.to_lowercase();
    for room in rooms {
        let result = match op {
            Op::Append => {
                client
                    .append_to_room_metadata_array(room, field, &addr)
                    .await
            }
            Op::Remove => {
                client
                    .remove_from_room_metadata_array(room, field, &addr)
                    .await
            }
        };
        if let Err(error) = result {
            tracing::warn!(
                %error,
                room = %room,
                field,
                address = %addr,
                "failed to sync scene room metadata (best-effort)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{context_matches_place, plan, PlaceInfo, RoomContext, RoomPlan};
    use serde_json::json;

    fn ctx(realm: Option<&str>, scene: Option<&str>) -> RoomContext {
        RoomContext {
            place_id: "place-1".into(),
            realm_name: realm.map(str::to_string),
            scene_id: scene.map(str::to_string),
            parcel: None,
        }
    }

    fn ctx_at(realm: Option<&str>, parcel: Option<&str>) -> RoomContext {
        RoomContext {
            parcel: parcel.map(str::to_string),
            ..ctx(realm, Some("bafkreiabc"))
        }
    }

    fn genesis_place(positions: &[&str]) -> PlaceInfo {
        PlaceInfo {
            world: false,
            world_name: None,
            positions: positions.iter().map(|p| p.to_string()).collect(),
            base_position: positions.first().map(|p| p.to_string()),
        }
    }

    fn world_place(name: &str) -> PlaceInfo {
        PlaceInfo {
            world: true,
            world_name: Some(name.into()),
            positions: vec!["0,0".into()],
            base_position: Some("0,0".into()),
        }
    }

    #[test]
    fn genesis_city_place_plans_the_scene_room() {
        assert_eq!(
            plan(&ctx(Some("main"), Some("bafkreiabc"))),
            RoomPlan::Scene("scene:main:bafkreiabc".into())
        );
    }

    #[test]
    fn world_place_plans_the_world_rooms() {
        assert_eq!(
            plan(&ctx(Some("foo.dcl.eth"), Some("bafkreiabc"))),
            RoomPlan::World {
                world: "foo.dcl.eth".into(),
                scene_id: Some("bafkreiabc".into()),
            }
        );
        assert_eq!(
            plan(&ctx(Some("foo.dcl.eth"), None)),
            RoomPlan::World {
                world: "foo.dcl.eth".into(),
                scene_id: None,
            }
        );
    }

    #[test]
    fn preview_realms_plan_the_scene_room_upstream_names() {
        for realm in ["localpreview", "preview", "LocalPreview", "PREVIEW"] {
            assert_eq!(
                plan(&ctx(Some(realm), Some("b64-scene"))),
                RoomPlan::Scene(format!("scene:{realm}:b64-scene")),
                "upstream skips previews only on the webhook refresh path"
            );
        }
    }

    #[test]
    fn a_world_context_must_name_the_authorized_world() {
        assert!(context_matches_place(
            &ctx_at(Some("Foo.dcl.eth"), None),
            &world_place("foo.dcl.eth")
        ));
        assert!(!context_matches_place(
            &ctx_at(Some("other.dcl.eth"), None),
            &world_place("foo.dcl.eth")
        ));
        assert!(!context_matches_place(
            &ctx_at(Some("foo.dcl.eth"), Some("10,20")),
            &genesis_place(&["10,20"])
        ));
    }

    #[test]
    fn a_genesis_context_must_stand_on_the_authorized_place() {
        assert!(context_matches_place(
            &ctx_at(Some("main"), Some("10,21")),
            &genesis_place(&["10,20", "10,21"])
        ));
        assert!(!context_matches_place(
            &ctx_at(Some("main"), Some("99,99")),
            &genesis_place(&["10,20", "10,21"])
        ));
        assert!(!context_matches_place(
            &ctx_at(Some("main"), None),
            &genesis_place(&["10,20"])
        ));
        assert!(!context_matches_place(
            &ctx_at(Some("main"), Some("0,0")),
            &world_place("foo.dcl.eth")
        ));
    }

    #[test]
    fn a_request_naming_no_realm_has_nothing_to_cross_check() {
        assert!(context_matches_place(
            &ctx_at(None, None),
            &genesis_place(&["10,20"])
        ));
    }

    #[test]
    fn a_non_world_realm_without_a_scene_id_names_no_room() {
        assert_eq!(plan(&ctx(Some("main"), None)), RoomPlan::Skip);
    }

    #[test]
    fn a_request_naming_no_realm_falls_back_to_the_place_record() {
        assert_eq!(plan(&ctx(None, Some("bafkreiabc"))), RoomPlan::FromPlace);
        assert_eq!(plan(&ctx(None, None)), RoomPlan::FromPlace);
    }

    #[test]
    fn context_reads_the_realm_and_scene_upstream_signs_over() {
        let flat = RoomContext::from_metadata(
            &json!({ "realmName": "main", "sceneId": "bafkreiabc" }),
            "place-1",
        );
        assert_eq!(plan(&flat), RoomPlan::Scene("scene:main:bafkreiabc".into()));

        let nested = RoomContext::from_metadata(
            &json!({ "realm": { "serverName": "main" }, "sceneId": "bafkreiabc" }),
            "place-1",
        );
        assert_eq!(
            plan(&nested),
            RoomPlan::Scene("scene:main:bafkreiabc".into())
        );

        let empty = RoomContext::from_metadata(&json!({}), "place-1");
        assert_eq!(plan(&empty), RoomPlan::FromPlace);

        let with_parcel = RoomContext::from_metadata(
            &json!({ "realmName": "main", "sceneId": "bafkreiabc", "parcel": "10,20" }),
            "place-1",
        );
        assert_eq!(with_parcel.parcel.as_deref(), Some("10,20"));
    }
}
