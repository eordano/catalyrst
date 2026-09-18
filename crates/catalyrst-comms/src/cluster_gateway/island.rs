use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::assignment_fence::TokenFence;
use crate::cluster_subscriber::{is_session_key, GatewayError, IslandOccupant};
use crate::livekit::{
    build_adapter_url, join_grants, with_not_before, AccessToken, ParticipantInfo,
    RoomServiceClient,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct IslandMetadata {
    catalyrst_island: Owner,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Owner {
    version: u8,
    wallet: String,
    session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assignment_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fencing_token: Option<u64>,
}

pub fn mint_island_connection_string(
    api_key: &str,
    api_secret: &str,
    ws_url: &str,
    wallet: &str,
    session: Option<&str>,
    room: &str,
    ttl_seconds: u64,
    not_before_unix: Option<u64>,
) -> Result<String, GatewayError> {
    mint_island_connection_string_inner(
        api_key,
        api_secret,
        ws_url,
        wallet,
        session,
        room,
        ttl_seconds,
        not_before_unix,
        None,
    )
}

pub fn mint_fenced_island_connection_string(
    api_key: &str,
    api_secret: &str,
    ws_url: &str,
    wallet: &str,
    room: &str,
    ttl_seconds: u64,
    fence: &TokenFence,
) -> Result<String, GatewayError> {
    if fence.owner_session.is_empty()
        || fence.owner_epoch == 0
        || fence.assignment_revision == 0
        || fence.fencing_token == 0
        || fence.audience.is_empty()
        || fence.lane != "realm"
    {
        return Err(GatewayError::new("invalid assignment fence"));
    }
    mint_island_connection_string_inner(
        api_key,
        api_secret,
        ws_url,
        wallet,
        Some(&fence.owner_session),
        room,
        ttl_seconds,
        None,
        Some(fence),
    )
}

#[allow(clippy::too_many_arguments)]
fn mint_island_connection_string_inner(
    api_key: &str,
    api_secret: &str,
    ws_url: &str,
    wallet: &str,
    session: Option<&str>,
    room: &str,
    ttl_seconds: u64,
    not_before_unix: Option<u64>,
    fence: Option<&TokenFence>,
) -> Result<String, GatewayError> {
    let mut grants = join_grants(room);
    grants.can_update_own_metadata = false;
    let mut token = AccessToken::new(api_key, api_secret, wallet, grants)
        .with_ttl(Duration::from_secs(ttl_seconds));
    if let Some(session) = session {
        if !is_session_key(wallet) || !is_session_key(session) {
            return Err(GatewayError::new("invalid island owner"));
        }
        let metadata = IslandMetadata {
            catalyrst_island: Owner {
                version: if fence.is_some() { 2 } else { 1 },
                wallet: wallet.into(),
                session: session.into(),
                audience: fence.map(|value| value.audience.clone()),
                lane: fence.map(|value| value.lane.clone()),
                owner_epoch: fence.map(|value| value.owner_epoch),
                assignment_revision: fence.map(|value| value.assignment_revision),
                fencing_token: fence.map(|value| value.fencing_token),
            },
        };
        token = token.with_metadata(
            serde_json::to_string(&metadata)
                .map_err(|_| GatewayError::new("cannot encode island owner"))?,
        );
    }
    let token = token
        .to_jwt()
        .map_err(|_| GatewayError::new("cannot mint island token"))?;
    let token = match not_before_unix {
        Some(not_before) => with_not_before(&token, api_secret, not_before)
            .map_err(|_| GatewayError::new("cannot set island token boundary"))?,
        None => token,
    };
    Ok(build_adapter_url(ws_url, &token))
}

pub async fn island_occupant(
    service: &RoomServiceClient<'_>,
    room: &str,
    wallet: &str,
) -> Result<IslandOccupant, GatewayError> {
    let participant = service
        .get_participant(room, wallet)
        .await
        .map_err(|_| GatewayError::new("cannot inspect island participant"))?;
    let Some(participant) = participant else {
        return Ok(IslandOccupant::Absent);
    };
    Ok(occupant_from_participant(&participant, wallet))
}

fn occupant_from_participant(participant: &ParticipantInfo, wallet: &str) -> IslandOccupant {
    if participant.identity != wallet
        || participant.sid.is_empty()
        || participant.can_update_metadata != Some(false)
    {
        return IslandOccupant::Unattributed;
    }
    owner_from_metadata(participant.metadata.as_deref(), wallet)
        .map(IslandOccupant::Session)
        .unwrap_or(IslandOccupant::Unattributed)
}

fn owner_from_metadata(metadata: Option<&str>, wallet: &str) -> Option<String> {
    let metadata = metadata.filter(|m| m.len() <= 4096)?;
    let owner = serde_json::from_str::<IslandMetadata>(metadata)
        .ok()?
        .catalyrst_island;
    let valid_fence = match owner.version {
        1 => {
            owner.audience.is_none()
                && owner.lane.is_none()
                && owner.owner_epoch.is_none()
                && owner.assignment_revision.is_none()
                && owner.fencing_token.is_none()
        }
        2 => {
            owner
                .audience
                .as_ref()
                .is_some_and(|value| !value.is_empty())
                && owner.lane.as_deref() == Some("realm")
                && owner.owner_epoch.is_some_and(|value| value > 0)
                && owner.assignment_revision.is_some_and(|value| value > 0)
                && owner.fencing_token.is_some_and(|value| value > 0)
        }
        _ => false,
    };
    (valid_fence && owner.wallet == wallet && is_session_key(&owner.session))
        .then_some(owner.session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    const WALLET: &str = "0x1111111111111111111111111111111111111111";
    const SESSION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn island_tokens_bind_a_session_and_disallow_self_metadata_updates() {
        let adapter = mint_island_connection_string(
            "key",
            "secret",
            "wss://sfu",
            WALLET,
            Some(SESSION),
            "island-test",
            60,
            Some(1700000000),
        )
        .unwrap();
        let token = adapter.split_once("access_token=").unwrap().1;
        let payload: serde_json::Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(token.split('.').nth(1).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload["sub"], WALLET);
        assert_eq!(payload["video"]["canUpdateOwnMetadata"], false);
        assert_eq!(payload["video"]["room"], "island-test");
        assert_eq!(payload["nbf"], 1700000000);
        assert_eq!(
            owner_from_metadata(payload["metadata"].as_str(), WALLET),
            Some(SESSION.into())
        );
    }

    #[test]
    fn fenced_island_tokens_carry_the_authoritative_owner_and_revision() {
        let fence = TokenFence {
            audience: "deployment-a".into(),
            lane: "realm".into(),
            owner_session: SESSION.into(),
            owner_epoch: 7,
            assignment_revision: 12,
            fencing_token: 3,
        };
        let adapter = mint_fenced_island_connection_string(
            "key",
            "secret",
            "wss://sfu",
            WALLET,
            "island-test",
            60,
            &fence,
        )
        .unwrap();
        let token = adapter.split_once("access_token=").unwrap().1;
        let payload: serde_json::Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(token.split('.').nth(1).unwrap())
                .unwrap(),
        )
        .unwrap();
        let metadata: serde_json::Value =
            serde_json::from_str(payload["metadata"].as_str().unwrap()).unwrap();
        assert_eq!(metadata["catalyrstIsland"]["version"], 2);
        assert_eq!(metadata["catalyrstIsland"]["audience"], "deployment-a");
        assert_eq!(metadata["catalyrstIsland"]["ownerEpoch"], 7);
        assert_eq!(metadata["catalyrstIsland"]["assignmentRevision"], 12);
        assert_eq!(metadata["catalyrstIsland"]["fencingToken"], 3);
        assert_eq!(
            owner_from_metadata(payload["metadata"].as_str(), WALLET),
            Some(SESSION.into())
        );
    }

    #[test]
    fn unknown_or_malformed_metadata_never_authorizes_replacement() {
        for value in [
            None,
            Some(""),
            Some("{}"),
            Some("not-json"),
            Some(r#"{"catalyrstIsland":{"version":1,"wallet":"wrong","session":"wrong"}}"#),
        ] {
            assert!(owner_from_metadata(value, WALLET).is_none());
        }
        let valid =
            serde_json::json!({"catalyrstIsland": {"version":1,"wallet":WALLET,"session":SESSION}});
        for field in ["version", "wallet", "session"] {
            let mut invalid = valid.clone();
            invalid["catalyrstIsland"][field] = serde_json::json!("invalid");
            assert!(owner_from_metadata(Some(&invalid.to_string()), WALLET).is_none());
        }
        assert!(mint_island_connection_string(
            "key",
            "secret",
            "wss://sfu",
            WALLET,
            Some("legacy"),
            "island-test",
            60,
            None
        )
        .is_err());
    }

    #[test]
    fn only_locked_well_formed_participant_ownership_is_usable() {
        for invalid in [
            "none",
            "writable",
            "unknown-permission",
            "identity",
            "sid",
            "metadata",
        ] {
            let mut participant = ParticipantInfo {
                sid: "PA_fixture".into(),
                identity: WALLET.into(),
                name: None,
                state: 0,
                metadata: Some(
                    serde_json::json!({"catalyrstIsland": {
                        "version":1, "wallet":WALLET, "session":SESSION,
                    }})
                    .to_string(),
                ),
                is_publisher: false,
                can_update_metadata: Some(false),
            };
            match invalid {
                "writable" => participant.can_update_metadata = Some(true),
                "unknown-permission" => participant.can_update_metadata = None,
                "identity" => participant.identity = SESSION.into(),
                "sid" => participant.sid.clear(),
                "metadata" => participant.metadata = None,
                _ => {}
            }
            assert_eq!(
                occupant_from_participant(&participant, WALLET),
                if invalid == "none" {
                    IslandOccupant::Session(SESSION.into())
                } else {
                    IslandOccupant::Unattributed
                },
                "{invalid}"
            );
        }
    }
}
