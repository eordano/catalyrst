pub use catalyrst_livekit::{
    build_adapter_url, ingress_admin_token, room_admin_token, verify_webhook_token, AccessToken,
    LivekitError, VideoGrants, TRACK_SOURCE_MICROPHONE,
};

/// The comms join grant: full publish/subscribe, metadata self-writes allowed
/// (the gatekeeper re-stamps metadata through the room service), no room list.
pub fn join_grants(room: impl Into<String>) -> VideoGrants {
    VideoGrants {
        room_join: true,
        room: room.into(),
        can_publish: true,
        can_subscribe: true,
        can_publish_data: true,
        can_update_own_metadata: true,
        room_list: Some(false),
        can_publish_sources: None,
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    fn decode_payload(jwt: &str) -> serde_json::Value {
        let payload_b64 = jwt.split('.').nth(1).unwrap();
        let bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn metadata_and_locked_grant_survive_in_jwt() {
        let mut grants = join_grants("scene:test");
        grants.can_update_own_metadata = false;
        let jwt = AccessToken::new("key", "secret", "0xabc", grants)
            .with_metadata(serde_json::json!({ "isGuest": false }).to_string())
            .to_jwt()
            .unwrap();
        let payload = decode_payload(&jwt);
        assert_eq!(payload["metadata"], "{\"isGuest\":false}");
        assert_eq!(payload["video"]["canUpdateOwnMetadata"], false);
        assert_eq!(payload["sub"], "0xabc");
    }

    #[test]
    fn join_grants_keep_the_comms_room_list_claim() {
        let jwt = AccessToken::new("key", "secret", "0xabc", join_grants("room1"))
            .to_jwt()
            .unwrap();
        let payload = decode_payload(&jwt);
        assert_eq!(payload["video"]["roomList"], false);
    }
}
