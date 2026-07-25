use crate::livekit::{
    world_room_name, world_scene_room_name, BANNED_ADDRESSES_FIELD, SCENE_ADMINS_FIELD,
};
use crate::ports::extra_addresses::load_place_info;
use crate::AppState;

pub async fn add_ban(state: &AppState, place_id: &str, address: &str) {
    mutate(state, place_id, BANNED_ADDRESSES_FIELD, address, Op::Append).await;
}

pub async fn remove_ban(state: &AppState, place_id: &str, address: &str) {
    mutate(state, place_id, BANNED_ADDRESSES_FIELD, address, Op::Remove).await;
}

pub async fn add_admin(state: &AppState, place_id: &str, address: &str) {
    mutate(state, place_id, SCENE_ADMINS_FIELD, address, Op::Append).await;
}

pub async fn remove_admin(state: &AppState, place_id: &str, address: &str) {
    mutate(state, place_id, SCENE_ADMINS_FIELD, address, Op::Remove).await;
}

#[derive(Clone, Copy)]
enum Op {
    Append,
    Remove,
}

async fn mutate(state: &AppState, place_id: &str, field: &str, address: &str, op: Op) {
    if !state.livekit_configured {
        return;
    }
    let rooms = scene_room_names_for_place(state, place_id).await;
    if rooms.is_empty() {
        return;
    }
    let client = state.room_service();
    let addr = address.to_lowercase();
    for room in rooms {
        let result = match op {
            Op::Append => {
                client
                    .append_to_room_metadata_array(&room, field, &addr)
                    .await
            }
            Op::Remove => {
                client
                    .remove_from_room_metadata_array(&room, field, &addr)
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

async fn scene_room_names_for_place(state: &AppState, place_id: &str) -> Vec<String> {
    let Some(place) = load_place_info(state, place_id).await else {
        return Vec::new();
    };
    let mut rooms = Vec::new();
    if place.world {
        if let Some(world_name) = place.world_name.as_deref() {
            rooms.push(world_room_name(world_name));
            if let Some(scene_id) =
                crate::handlers::scene_adapter::fetch_world_scene_id(state, world_name).await
            {
                rooms.push(world_scene_room_name(world_name, &scene_id));
            }
        }
    }
    rooms
}
