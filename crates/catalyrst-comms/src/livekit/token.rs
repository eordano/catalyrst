pub use catalyrst_livekit::{
    build_adapter_url, ingress_admin_token, room_admin_token, sign_hs256, verify_webhook_token,
    AccessToken, LivekitError, VideoGrants, TRACK_SOURCE_MICROPHONE,
};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// Re-signs a token [`AccessToken`] built, replacing only its `nbf`.
///
/// The minter stamps `nbf` with the mint instant and offers no way to set it, so the payload is
/// kept exactly as built (issuer, subject, expiry, grants) and signed the way the minter signs:
/// HS256 over the same header and payload with the API secret.
///
/// On LiveKit Cloud, revocation compares `nbf` at second granularity: a
/// replacement token minted in the same second as the one being revoked would be revoked with it
/// unless its `nbf` is moved forward to the boundary.
/// Self-hosted LiveKit does not enforce that cutoff; changing `nbf` is not a revocation mechanism.
pub fn with_not_before(jwt: &str, api_secret: &str, not_before_unix: u64) -> Result<String> {
    let mut parts = jwt.split('.');
    let header_b64 = parts.next().ok_or_else(|| anyhow!("token has no header"))?;
    let payload_b64 = parts
        .next()
        .ok_or_else(|| anyhow!("token has no payload"))?;
    let header = URL_SAFE_NO_PAD
        .decode(header_b64)
        .context("token header is not base64url")?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .context("token payload is not base64url")?;
    let mut claims: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&payload).context("token payload is not a json object")?;
    claims.insert("nbf".into(), serde_json::json!(not_before_unix));
    Ok(sign_hs256(
        api_secret,
        &header,
        &serde_json::to_vec(&serde_json::Value::Object(claims))?,
    )?)
}

/// The comms join grant: full publish/subscribe, metadata self-writes allowed
/// (the gatekeeper re-stamps metadata through the room service), no room list.
/// `can_publish_sources` is restricted to microphone so a routine signed-fetch
/// token can never publish camera or screen-share tracks into scene or
/// community rooms (RTMP streams go through ingress admin tokens, not this
/// grant).
pub fn join_grants(room: impl Into<String>) -> VideoGrants {
    VideoGrants {
        room_join: true,
        room: room.into(),
        can_publish: true,
        can_subscribe: true,
        can_publish_data: true,
        can_update_own_metadata: true,
        room_list: Some(false),
        can_publish_sources: Some(vec![TRACK_SOURCE_MICROPHONE.to_string()]),
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
