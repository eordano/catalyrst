pub const WORLD_ROOM_PREFIX: &str = "world-";
pub const PRIVATE_VOICE_CHAT_ROOM_PREFIX: &str = "voice-chat-private-";
pub const COMMUNITY_VOICE_CHAT_ROOM_PREFIX: &str = "voice-chat-community";

pub fn private_voice_chat_room_name(room_id: &str) -> String {
    format!("{}{}", PRIVATE_VOICE_CHAT_ROOM_PREFIX, room_id)
}

pub fn is_private_voice_chat_room(room_name: &str) -> bool {
    room_name.starts_with(PRIVATE_VOICE_CHAT_ROOM_PREFIX)
}

pub fn is_community_voice_chat_room(room_name: &str) -> bool {
    room_name.starts_with(&format!("{}-", COMMUNITY_VOICE_CHAT_ROOM_PREFIX))
}

pub fn community_voice_chat_room_name(community_id: &str) -> String {
    format!("{}-{}", COMMUNITY_VOICE_CHAT_ROOM_PREFIX, community_id)
}

pub fn community_id_from_room_name(room_name: &str) -> String {
    room_name
        .strip_prefix(&format!("{}-", COMMUNITY_VOICE_CHAT_ROOM_PREFIX))
        .unwrap_or(room_name)
        .to_string()
}

pub fn scene_room_name(scene_id: &str) -> String {
    format!("scene:{scene_id}")
}

pub fn world_scene_room_name(world: &str, scene_id: &str) -> String {
    format!("{}{}-{}", WORLD_ROOM_PREFIX, world, scene_id)
}

pub fn world_room_name(world: &str) -> String {
    format!("{}{}", WORLD_ROOM_PREFIX, world)
}

pub fn build_adapter_url(host: &str, token: &str) -> String {
    let host = if host.starts_with("wss://") || host.starts_with("ws://") {
        host.to_string()
    } else {
        format!("wss://{}", host)
    };
    format!("livekit:{}?access_token={}", host, token)
}

pub fn address_from_identity(identity: &str) -> Option<String> {
    let lower = identity.to_lowercase();
    let candidate: String = lower.chars().take(42).collect();
    if candidate.len() == 42
        && candidate.starts_with("0x")
        && candidate[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        Some(candidate)
    } else {
        None
    }
}

pub fn room_service_base(host: &str) -> String {
    let insecure = host.starts_with("ws://") || host.starts_with("http://");
    let trimmed = host
        .trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let scheme = if insecure { "http" } else { "https" };
    format!("{scheme}://{trimmed}")
}
