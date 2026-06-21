//! Resolves a profile **entity id** to its embedded avatar wire-format JSON by
//! reading the entity manifest from the local catalyrst content core.
//!
//! Upstream's render pipeline (`local_entity_snapshot.sh`) does exactly this:
//! it GETs `<content>/contents/<cid>`, parses the deployment manifest, and
//! pulls `metadata.avatars[0].avatar` as the renderer payload. We talk to the
//! local content core (`catalyrst` :5141, `/content`) instead of prod so a
//! self-hosted realm never leaks to `peer.decentraland.org`.

use std::time::Duration;

use serde_json::Value;

/// Outcome of resolving an entity id to its avatar payload.
pub enum ResolveResult {
    /// The avatar wire-format object (`metadata.avatars[0].avatar`).
    Avatar(Value),
    /// The content core has no such entity (404) or it is not a profile /
    /// carries no avatar. The caller should treat this as "no image".
    NotFound,
    /// Transport / parse failure talking to the content core.
    Error(String),
}

/// Reads profile manifests from a catalyst content server.
pub struct ProfileResolver {
    client: reqwest::Client,
    /// Content base **including** the `/content` suffix, e.g.
    /// `http://127.0.0.1:5141/content`. This is also handed to the renderer as
    /// `baseUrl` so its wearable lookups hit the same server.
    content_base: String,
}

impl ProfileResolver {
    pub fn new(content_base: String) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("catalyrst-profile-images/0.1")
            .build()
            .expect("reqwest client");
        Self {
            client,
            content_base: content_base.trim_end_matches('/').to_string(),
        }
    }

    /// The content base URL the renderer should use for wearable fetches.
    pub fn content_base(&self) -> &str {
        &self.content_base
    }

    /// Fetch the entity manifest for `entity` and extract the avatar payload.
    pub async fn resolve(&self, entity: &str) -> ResolveResult {
        let url = format!("{}/contents/{}", self.content_base, entity);
        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return ResolveResult::Error(format!("content request failed: {e}")),
        };
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return ResolveResult::NotFound;
        }
        if !resp.status().is_success() {
            return ResolveResult::Error(format!("content status {}", resp.status()));
        }
        let manifest: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => return ResolveResult::Error(format!("content json parse failed: {e}")),
        };
        extract_avatar(&manifest)
    }
}

/// Pull `metadata.avatars[0].avatar` out of a deployment manifest. Mirrors the
/// classification upstream's `local_entity_snapshot.sh` does in its payload
/// builder (not-a-profile / no-avatars / missing-avatar-field all collapse to
/// `NotFound` here since the client contract is "404 = no image").
fn extract_avatar(manifest: &Value) -> ResolveResult {
    // Reject non-profile entities explicitly when the type field is present.
    if let Some(t) = manifest.get("type").and_then(Value::as_str) {
        if t != "profile" {
            return ResolveResult::NotFound;
        }
    }
    let avatar = manifest
        .get("metadata")
        .and_then(|m| m.get("avatars"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|a0| a0.get("avatar"))
        .cloned();
    match avatar {
        Some(v) if v.is_object() => ResolveResult::Avatar(v),
        _ => ResolveResult::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_avatar_from_profile_manifest() {
        let m = json!({
            "type": "profile",
            "metadata": { "avatars": [ { "avatar": { "bodyShape": "x", "wearables": [] } } ] }
        });
        match extract_avatar(&m) {
            ResolveResult::Avatar(v) => assert_eq!(v["bodyShape"], "x"),
            _ => panic!("expected avatar"),
        }
    }

    #[test]
    fn non_profile_is_not_found() {
        let m = json!({ "type": "scene", "metadata": {} });
        assert!(matches!(extract_avatar(&m), ResolveResult::NotFound));
    }

    #[test]
    fn missing_avatar_is_not_found() {
        let m = json!({ "type": "profile", "metadata": { "avatars": [] } });
        assert!(matches!(extract_avatar(&m), ResolveResult::NotFound));
    }
}
