use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use catalyrst_pulse::application_relay::auth::{address_bytes, VerifiedRoom};
use serde::Deserialize;

use super::{DenialReason, RelayPolicy};
use crate::{
    assignment_fence::{FenceError, RealmAssignmentReader, RealmAssignmentSnapshot},
    ports::extra_addresses::world_access_allowed_bytes,
    AppState,
};

pub struct CurrentRoomPolicy {
    state: AppState,
    assignments: Option<Arc<dyn RealmAssignmentReader>>,
    http: reqwest::Client,
}

impl CurrentRoomPolicy {
    pub fn new(
        state: AppState,
        assignments: Option<Arc<dyn RealmAssignmentReader>>,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            state,
            assignments,
            http: reqwest::Client::builder()
                .timeout(Duration::from_millis(500))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }

    async fn world_document(&self, world: &str, resource: &str) -> Result<Vec<u8>, DenialReason> {
        let url = format!(
            "{}/world/{}/{}",
            self.state.world_content_url.trim_end_matches('/'),
            crate::http::encode_path_segment(world),
            resource,
        );
        let mut response = self
            .http
            .get(url)
            .header(reqwest::header::CACHE_CONTROL, "no-cache, no-store")
            .send()
            .await
            .map_err(|_| DenialReason::Unavailable)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > 65_536)
        {
            return Err(DenialReason::Unavailable);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| DenialReason::Unavailable)?
        {
            if body.len() + chunk.len() > 65_536 {
                return Err(DenialReason::Unavailable);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn scene_gate(&self, wallet: &str, scene: &str) -> Result<(), DenialReason> {
        match self
            .state
            .user_bans
            .connection_gate(wallet, None, scene)
            .await
        {
            Ok((false, false)) => Ok(()),
            Ok(_) => Err(DenialReason::NotAuthorized),
            Err(_) => Err(DenialReason::Unavailable),
        }
    }
}

#[async_trait]
impl RelayPolicy for CurrentRoomPolicy {
    async fn authorize(&self, room: &VerifiedRoom) -> Result<(), DenialReason> {
        let scope = scope(&room.claims.video.room).ok_or(DenialReason::NotAuthorized)?;
        let metadata: serde_json::Value = if room.claims.metadata.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&room.claims.metadata).map_err(|_| DenialReason::Invalid)?
        };
        if !matches!(scope, Scope::Island(_)) && metadata.get("catalyrstIsland").is_some() {
            return Err(DenialReason::NotAuthorized);
        }
        let principal = if room.claims.sub == "authoritative-server" {
            if matches!(scope, Scope::Island(_)) {
                return Err(DenialReason::NotAuthorized);
            }
            let expected = self
                .state
                .authoritative_server_address
                .as_deref()
                .and_then(address_bytes)
                .ok_or(DenialReason::NotAuthorized)?;
            let author: AuthoritativeServer = serde_json::from_value(
                metadata
                    .get("catalyrstAuthoritativeServer")
                    .cloned()
                    .ok_or(DenialReason::NotAuthorized)?,
            )
            .map_err(|_| DenialReason::NotAuthorized)?;
            if address_bytes(&author.wallet) != Some(expected) {
                return Err(DenialReason::NotAuthorized);
            }
            author.wallet.to_ascii_lowercase()
        } else {
            if metadata.get("catalyrstAuthoritativeServer").is_some() {
                return Err(DenialReason::NotAuthorized);
            }
            room.wallet.clone()
        };
        if principal != room.wallet {
            match self
                .state
                .user_bans
                .is_banned_for_connection(&room.wallet, None)
                .await
            {
                Ok(false) => {}
                Ok(true) => return Err(DenialReason::NotAuthorized),
                Err(_) => return Err(DenialReason::Unavailable),
            }
        }
        match scope {
            Scope::Island(island) => {
                let owner: IslandMetadata = serde_json::from_str(&room.claims.metadata)
                    .map_err(|_| DenialReason::NotAuthorized)?;
                let reader = self.assignments.as_ref().ok_or(DenialReason::Unavailable)?;
                let current = reader
                    .current_realm_assignment(&room.wallet, &room.session)
                    .await
                    .map_err(|error| match error {
                        FenceError::Unavailable => DenialReason::Unavailable,
                        _ => DenialReason::NotAuthorized,
                    })?;
                if !owner.catalyrst_island.matches(room, island, &current) {
                    return Err(DenialReason::NotAuthorized);
                }
                match self
                    .state
                    .user_bans
                    .is_banned_for_connection(&room.wallet, None)
                    .await
                {
                    Ok(false) => Ok(()),
                    Ok(true) => Err(DenialReason::NotAuthorized),
                    Err(_) => Err(DenialReason::Unavailable),
                }
            }
            Scope::Scene(scene) => self.scene_gate(&principal, scene).await,
            Scope::World { world, scene } => {
                let world = world.to_ascii_lowercase();
                let permissions = self.world_document(&world, "permissions").await?;
                if !world_access_allowed_bytes(&permissions, &principal) {
                    return Err(DenialReason::NotAuthorized);
                }
                self.scene_gate(&principal, &world).await?;
                match scene {
                    Some(scene) => self.scene_gate(&principal, scene).await,
                    None => {
                        let body = self.world_document(&world, "about").await?;
                        let scene = single_world_scene(&body).ok_or(DenialReason::NotAuthorized)?;
                        self.scene_gate(&principal, &scene).await
                    }
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoritativeServer {
    wallet: String,
}

#[derive(Debug, PartialEq)]
enum Scope<'a> {
    Island(&'a str),
    Scene(&'a str),
    World {
        world: &'a str,
        scene: Option<&'a str>,
    },
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

fn world_name(value: &str) -> bool {
    value.strip_suffix(".eth").is_some_and(|name| {
        !name.is_empty()
            && name.len() <= 128
            && name.split('.').all(identifier)
            && !name.contains(".eth-")
    })
}

fn scope(room: &str) -> Option<Scope<'_>> {
    if let Some(island) = room.strip_prefix("island-") {
        return identifier(island).then_some(Scope::Island(room));
    }
    if let Some(scene) = room.strip_prefix("scene:") {
        let (realm, scene) = scene.split_once(':')?;
        return (!realm.is_empty()
            && realm.len() <= 128
            && !realm.ends_with(".eth")
            && !realm.chars().any(char::is_control)
            && identifier(scene))
        .then_some(Scope::Scene(scene));
    }
    if let Some(world) = room.strip_prefix("world-") {
        if world_name(world) {
            return Some(Scope::World { world, scene: None });
        }
        let (name, scene) = world.split_once(".eth-")?;
        let name = &world[..name.len() + 4];
        return (world_name(name) && identifier(scene)).then_some(Scope::World {
            world: name,
            scene: Some(scene),
        });
    }
    None
}

fn single_world_scene(body: &[u8]) -> Option<String> {
    let about: serde_json::Value = serde_json::from_slice(body).ok()?;
    let urns = about.get("configurations")?.get("scenesUrn")?.as_array()?;
    if urns.len() != 1 {
        return None;
    }
    let scene = urns[0]
        .as_str()?
        .strip_prefix("urn:decentraland:entity:")?
        .split('?')
        .next()?;
    identifier(scene).then(|| scene.to_owned())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct IslandMetadata {
    catalyrst_island: IslandOwner,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct IslandOwner {
    version: u8,
    wallet: String,
    session: String,
    audience: String,
    lane: String,
    owner_epoch: u64,
    assignment_revision: u64,
    fencing_token: u64,
}

impl IslandOwner {
    fn matches(
        &self,
        room: &VerifiedRoom,
        island: &str,
        current: &RealmAssignmentSnapshot,
    ) -> bool {
        self.version == 2
            && address_bytes(&self.wallet) == address_bytes(&room.wallet)
            && address_bytes(&self.session) == address_bytes(&room.session)
            && current.wallet == room.wallet
            && current.owner_session == room.session
            && self.audience == current.audience
            && self.lane == "realm"
            && self.lane == current.lane
            && self.owner_epoch > 0
            && self.owner_epoch == current.owner_epoch
            && self.assignment_revision > 0
            && self.assignment_revision == current.assignment_revision
            && self.fencing_token > 0
            && self.fencing_token == current.fencing_token
            && island == current.island_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_scope_rejects_ambiguous_or_unsupported_purposes() {
        assert_eq!(scope("island-123"), Some(Scope::Island("island-123")));
        assert_eq!(scope("scene:main:cid-123"), Some(Scope::Scene("cid-123")));
        assert_eq!(
            scope("world-foo-bar.eth-cid"),
            Some(Scope::World {
                world: "foo-bar.eth",
                scene: Some("cid"),
            })
        );
        assert_eq!(
            scope("world-foo.eth"),
            Some(Scope::World {
                world: "foo.eth",
                scene: None
            })
        );
        for name in [
            "island-",
            "scene:main:other:cid",
            "scene:foo.eth:cid",
            "scene::cid",
            "world-foo.eth-other.eth-cid",
            "world-foo.eth-../cid",
            "private-messages",
            "community-voice-chat-1",
            "unknown",
        ] {
            assert_eq!(scope(name), None, "{name}");
        }
    }

    #[test]
    fn world_root_requires_one_unambiguous_current_scene() {
        assert_eq!(
            single_world_scene(
                br#"{"configurations":{"scenesUrn":["urn:decentraland:entity:abc?baseUrl=x"]}}"#
            ),
            Some("abc".into())
        );
        for body in [br#"{"configurations":{"scenesUrn":[]}}"#.as_slice(),
            br#"{"configurations":{"scenesUrn":["urn:decentraland:entity:a","urn:decentraland:entity:b"]}}"#,
            br#"{"configurations":{"scenesUrn":["urn:decentraland:entity:../x"]}}"#] {
            assert_eq!(single_world_scene(body), None);
        }
    }
}
