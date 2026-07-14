use reqwest::Client;
use std::time::Duration;

#[derive(Clone)]
pub struct Gatekeeper {
    base_url: String,
    auth_token: Option<String>,
    http: Client,
}

#[derive(Debug, thiserror::Error)]
pub enum GatekeeperError {
    #[error("gatekeeper request failed: {0}")]
    Request(String),
    #[error("gatekeeper returned status {0}")]
    Status(u16),
    #[error("invalid gatekeeper {0}")]
    InvalidIdentifier(&'static str),
}

/// Validate and canonicalise a UUID identifier before it is interpolated into a privileged
/// Gatekeeper path that carries the admin bearer token (upstream #446 `requireUuid`). Returns the
/// canonical lowercased hyphenated form; `Uuid::parse_str` rejects anything that could smuggle a
/// path segment (`/`, `?`, `#`, whitespace) into the URL.
fn require_uuid(value: &str, label: &'static str) -> Result<String, GatekeeperError> {
    uuid::Uuid::parse_str(value)
        .map(|u| u.to_string())
        .map_err(|_| GatekeeperError::InvalidIdentifier(label))
}

/// Validate and lowercase an Ethereum address before interpolating it into a privileged path
/// (upstream #446 `requireAddress`). A valid `0x`-prefixed 40-hex address carries no URL delimiter.
fn require_address(value: &str) -> Result<String, GatekeeperError> {
    if catalyrst_types::is_eth_address(value) {
        Ok(value.to_lowercase())
    } else {
        Err(GatekeeperError::InvalidIdentifier("user address"))
    }
}

/// Percent-encode one path segment (upstream #446 `pathSegment`/`encodeURIComponent`). Belt-and-
/// suspenders after the format validators above: a canonical UUID or lowercased address contains
/// only unreserved bytes, so this is a no-op on validated input, but it keeps any future
/// unvalidated caller from injecting a delimiter.
fn path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Gatekeeper {
    pub fn new(base_url: String) -> Self {
        Self::with_token(base_url, std::env::var("COMMS_GATEKEEPER_AUTH_TOKEN").ok())
    }

    pub fn with_token(base_url: String, auth_token: Option<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_token: auth_token.filter(|s| !s.is_empty()),
            http,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, url);
        if let Some(tok) = &self.auth_token {
            req = req.bearer_auth(tok);
        }
        req
    }

    pub async fn private_voice_credentials(
        &self,
        room_id: &str,
        callee: &str,
        caller: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        let (room_id, callee, caller) = match (
            require_uuid(room_id, "private voice room ID"),
            require_address(callee),
            require_address(caller),
        ) {
            (Ok(r), Ok(callee), Ok(caller)) => (r, callee, caller),
            _ => {
                tracing::warn!("gatekeeper private-voice-chat: invalid room id or address");
                return out;
            }
        };
        let resp = match self
            .request(reqwest::Method::POST, "/private-voice-chat")
            .json(&serde_json::json!({
                "room_id": room_id,
                "user_addresses": [callee, caller],
            }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "gatekeeper private-voice-chat request failed");
                return out;
            }
        };
        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), "gatekeeper private-voice-chat non-success");
            return out;
        }

        let Ok(body) = resp.json::<serde_json::Value>().await else {
            return out;
        };
        if let Some(obj) = body.as_object() {
            for (addr, v) in obj {
                if let Some(u) = v.get("connection_url").and_then(|u| u.as_str()) {
                    out.insert(addr.to_lowercase(), u.to_string());
                }
            }
        }
        out
    }

    pub async fn is_user_in_a_voice_chat(&self, address: &str) -> Result<bool, GatekeeperError> {
        let address = require_address(address)?;
        let path = format!("/users/{}/voice-chat-status", path_segment(&address));
        let resp = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|e| GatekeeperError::Request(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(GatekeeperError::Status(resp.status().as_u16()));
        }
        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| GatekeeperError::Request(e.to_string()))?;
        Ok(body
            .get("is_user_in_voice_chat")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub async fn is_user_in_community_voice_chat(
        &self,
        address: &str,
    ) -> Result<bool, GatekeeperError> {
        let address = require_address(address)?;
        let path = format!(
            "/users/{}/community-voice-chat-status",
            path_segment(&address)
        );
        let resp = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|e| GatekeeperError::Request(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(GatekeeperError::Status(resp.status().as_u16()));
        }
        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| GatekeeperError::Request(e.to_string()))?;
        Ok(body
            .get("isInCommunityVoiceChat")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub async fn end_private_voice_chat(&self, call_id: &str, address: &str) {
        let (call_id, address) = match (
            require_uuid(call_id, "private voice call ID"),
            require_address(address),
        ) {
            (Ok(c), Ok(a)) => (c, a),
            _ => {
                tracing::warn!(
                    call_id,
                    "gatekeeper end private-voice-chat: invalid id or address"
                );
                return;
            }
        };
        let path = format!("/private-voice-chat/{}", path_segment(&call_id));
        if let Err(e) = self
            .request(reqwest::Method::DELETE, &path)
            .json(&serde_json::json!({ "address": address }))
            .send()
            .await
        {
            tracing::warn!(error = %e, call_id, "failed to end private voice chat");
        }
    }

    pub async fn community_voice_credentials(
        &self,
        community_id: &str,
        user_address: &str,
        user_role: &str,
        action: &str,
        profile: Option<serde_json::Value>,
    ) -> Option<String> {
        let community_id = require_uuid(community_id, "community ID")
            .map_err(|e| tracing::warn!(error = %e, "gatekeeper community-voice-chat: invalid id"))
            .ok()?;
        let user_address = require_address(user_address)
            .map_err(
                |e| tracing::warn!(error = %e, "gatekeeper community-voice-chat: invalid address"),
            )
            .ok()?;
        let mut body = serde_json::Map::new();
        body.insert("community_id".into(), serde_json::json!(community_id));
        body.insert("user_address".into(), serde_json::json!(user_address));
        body.insert("user_role".into(), serde_json::json!(user_role));
        body.insert("action".into(), serde_json::json!(action));
        if let Some(p) = profile {
            body.insert("profile_data".into(), p);
        }
        let resp = self
            .request(reqwest::Method::POST, "/community-voice-chat")
            .json(&serde_json::Value::Object(body))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), "gatekeeper community-voice-chat non-success");
            return None;
        }
        let body = resp.json::<serde_json::Value>().await.ok()?;
        body.get("connection_url")
            .and_then(|u| u.as_str())
            .map(String::from)
    }

    pub async fn end_community_voice_chat(
        &self,
        community_id: &str,
        user_address: &str,
    ) -> Result<(), GatekeeperError> {
        let community_id = require_uuid(community_id, "community ID")?;
        let user_address = require_address(user_address)?;
        let path = format!("/community-voice-chat/{}", path_segment(&community_id));
        self.fire(
            reqwest::Method::DELETE,
            &path,
            Some(serde_json::json!({ "user_address": user_address })),
        )
        .await
    }

    pub async fn request_to_speak(
        &self,
        community_id: &str,
        user_address: &str,
        raising_hand: bool,
    ) -> Result<(), GatekeeperError> {
        let community_id = require_uuid(community_id, "community ID")?;
        let user_address = require_address(user_address)?;
        let path = format!(
            "/community-voice-chat/{}/users/{}/speak-request",
            path_segment(&community_id),
            path_segment(&user_address)
        );
        let method = if raising_hand {
            reqwest::Method::POST
        } else {
            reqwest::Method::DELETE
        };
        self.fire(method, &path, None).await
    }

    pub async fn reject_speak_request(
        &self,
        community_id: &str,
        user_address: &str,
    ) -> Result<(), GatekeeperError> {
        let community_id = require_uuid(community_id, "community ID")?;
        let user_address = require_address(user_address)?;
        let path = format!(
            "/community-voice-chat/{}/users/{}/speak-request",
            path_segment(&community_id),
            path_segment(&user_address)
        );
        self.fire(reqwest::Method::DELETE, &path, None).await
    }

    pub async fn set_speaker(
        &self,
        community_id: &str,
        user_address: &str,
        promote: bool,
    ) -> Result<(), GatekeeperError> {
        let community_id = require_uuid(community_id, "community ID")?;
        let user_address = require_address(user_address)?;
        let path = format!(
            "/community-voice-chat/{}/users/{}/speaker",
            path_segment(&community_id),
            path_segment(&user_address)
        );
        let method = if promote {
            reqwest::Method::POST
        } else {
            reqwest::Method::DELETE
        };
        self.fire(method, &path, None).await
    }

    pub async fn kick_player(
        &self,
        community_id: &str,
        user_address: &str,
    ) -> Result<(), GatekeeperError> {
        let community_id = require_uuid(community_id, "community ID")?;
        let user_address = require_address(user_address)?;
        let path = format!(
            "/community-voice-chat/{}/users/{}",
            path_segment(&community_id),
            path_segment(&user_address)
        );
        self.fire(reqwest::Method::DELETE, &path, None).await
    }

    pub async fn mute_speaker(
        &self,
        community_id: &str,
        user_address: &str,
        muted: bool,
    ) -> Result<(), GatekeeperError> {
        let community_id = require_uuid(community_id, "community ID")?;
        let user_address = require_address(user_address)?;
        let path = format!(
            "/community-voice-chat/{}/users/{}/mute",
            path_segment(&community_id),
            path_segment(&user_address)
        );
        self.fire(
            reqwest::Method::PATCH,
            &path,
            Some(serde_json::json!({ "muted": muted })),
        )
        .await
    }

    async fn fire(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(), GatekeeperError> {
        let mut req = self.request(method, path);
        if let Some(b) = body {
            req = req.json(&b);
        }
        match req.send().await {
            Ok(resp) => {
                let code = resp.status().as_u16();

                if resp.status().is_success() || code == 404 {
                    Ok(())
                } else {
                    Err(GatekeeperError::Status(code))
                }
            }
            Err(e) => Err(GatekeeperError::Request(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_uuid_canonicalises_and_rejects_path_injection() {
        // A valid UUID normalises to canonical lowercased hyphenated form.
        assert_eq!(
            require_uuid("11111111-1111-4111-8111-111111111111", "community ID").unwrap(),
            "11111111-1111-4111-8111-111111111111"
        );
        assert_eq!(
            require_uuid("11111111111141118111111111111111", "community ID").unwrap(),
            "11111111-1111-4111-8111-111111111111"
        );
        // Anything carrying a URL delimiter or extra segment is refused before it reaches the path.
        for bad in [
            "../admin",
            "11111111-1111-4111-8111-111111111111/../x",
            "not-a-uuid",
            "",
        ] {
            assert!(require_uuid(bad, "community ID").is_err(), "{bad}");
        }
    }

    #[test]
    fn require_address_lowercases_and_rejects_non_addresses() {
        assert_eq!(
            require_address("0xAABBCCDDEEFF00112233445566778899AABBCCDD").unwrap(),
            "0xaabbccddeeff00112233445566778899aabbccdd"
        );
        for bad in ["0x123", "decentraland", "0xZZ", "", "0xaabb/../x"] {
            assert!(require_address(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn path_segment_is_a_noop_on_validated_ids_and_encodes_delimiters() {
        let uuid = "11111111-1111-4111-8111-111111111111";
        assert_eq!(path_segment(uuid), uuid);
        let addr = "0xaabbccddeeff00112233445566778899aabbccdd";
        assert_eq!(path_segment(addr), addr);
        assert_eq!(path_segment("a/b?c#d"), "a%2Fb%3Fc%23d");
    }
}
