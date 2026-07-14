pub use catalyrst_livekit::{build_adapter_url, AccessToken, LivekitError, VideoGrants};

/// The worlds join grant. Participant metadata is where the gatekeeper attests
/// `isGuest`, and clients read that bit to decide whether a peer is verified. A
/// participant that can rewrite its own metadata can present as verified, so
/// `can_update_own_metadata` must stay false -- matching the scene-adapter grant
/// in catalyrst-comms. `room_list`/`can_publish_sources` stay `None` so the
/// grant claim keeps the exact shape worlds has always minted.
pub fn join_grants(room: impl Into<String>) -> VideoGrants {
    VideoGrants {
        room_join: true,
        room: room.into(),
        can_publish: true,
        can_subscribe: true,
        can_publish_data: true,
        can_update_own_metadata: false,
        room_list: None,
        can_publish_sources: None,
    }
}

pub const WORLD_ROOM_PREFIX: &str = "world-";
pub const SCENE_ROOM_PREFIX: &str = "scene-";

pub fn world_room_name(world: &str) -> String {
    format!("{}{}", WORLD_ROOM_PREFIX, world.to_lowercase())
}

pub fn world_scene_room_name(world: &str, scene_id: &str) -> String {
    format!(
        "{}{}-{}",
        SCENE_ROOM_PREFIX,
        world.to_lowercase(),
        scene_id.to_lowercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    #[test]
    fn jwt_has_three_parts() {
        let tok = AccessToken::new("k", "s", "0xabc", join_grants("world-foo.eth"))
            .to_jwt()
            .unwrap();
        assert_eq!(tok.split('.').count(), 3);
    }

    #[test]
    fn join_grant_refuses_self_metadata_writes() {
        assert!(
            !join_grants("world-foo.eth").can_update_own_metadata,
            "a participant that can rewrite its own metadata can forge the attested isGuest bit"
        );
    }

    #[test]
    fn join_grant_claim_keeps_worlds_shape() {
        let jwt = AccessToken::new("k", "s", "0xabc", join_grants("world-foo.eth"))
            .to_jwt()
            .unwrap();
        let payload_b64 = jwt.split('.').nth(1).unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload_b64).unwrap()).unwrap();
        assert!(payload["video"].get("roomList").is_none());
        assert!(payload["video"].get("canPublishSources").is_none());
        assert!(payload.get("name").is_none());
        assert!(payload.get("metadata").is_none());
    }

    #[test]
    fn room_names_match_upstream() {
        assert_eq!(world_room_name("Foo.eth"), "world-foo.eth");
        assert_eq!(world_scene_room_name("Foo.eth", "ABC"), "scene-foo.eth-abc");
    }

    #[test]
    fn adapter_prefixes_wss() {
        assert!(build_adapter_url("lk.example.com", "t")
            .starts_with("livekit:wss://lk.example.com?access_token=t"));
    }
}
