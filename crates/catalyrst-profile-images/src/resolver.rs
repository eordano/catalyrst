use std::time::Duration;

use serde_json::Value;

pub enum ResolveResult {
    Avatar(Value),

    NotFound,

    Error(String),
}

pub struct ProfileResolver {
    client: reqwest::Client,

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

    pub fn content_base(&self) -> &str {
        &self.content_base
    }

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

fn extract_avatar(manifest: &Value) -> ResolveResult {
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
