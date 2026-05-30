use reqwest::Client;
use std::time::Duration;

/// HTTP client for the comms-gatekeeper (catalyrst-comms, default
/// `http://127.0.0.1:5138`). Endpoints + request/response shapes mirror
/// upstream `social-service-ea/src/adapters/comms-gatekeeper.ts`.
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

    /// POST /private-voice-chat — mints LiveKit tokens for both ends of a 1:1
    /// call in a single request. Returns a lowercase-address → connection-URL
    /// map (the gatekeeper response shape, camel-decoded), empty on failure.
    ///
    /// `room_id` is the social-service call id; the gatekeeper derives the
    /// `voice-chat-private-<room_id>` LiveKit room from it.
    pub async fn private_voice_credentials(
        &self,
        room_id: &str,
        callee: &str,
        caller: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
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
        // Body is { "<address>": { "connection_url": "livekit:..." }, ... }.
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

    /// DELETE /private-voice-chat/:id — tears down a 1:1 call room.
    pub async fn end_private_voice_chat(&self, call_id: &str, address: &str) {
        let path = format!("/private-voice-chat/{}", call_id);
        if let Err(e) = self
            .request(reqwest::Method::DELETE, &path)
            .json(&serde_json::json!({ "address": address }))
            .send()
            .await
        {
            tracing::warn!(error = %e, call_id, "failed to end private voice chat");
        }
    }

    /// POST /community-voice-chat — create-or-join a community voice room.
    /// `action` is "create" (moderator opening the room) or "join".
    /// Returns the connection (adapter) URL, or `None` on failure.
    pub async fn community_voice_credentials(
        &self,
        community_id: &str,
        user_address: &str,
        user_role: &str,
        action: &str,
        profile: Option<serde_json::Value>,
    ) -> Option<String> {
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

    /// DELETE /community-voice-chat/:id — force-end the community room.
    pub async fn end_community_voice_chat(
        &self,
        community_id: &str,
        user_address: &str,
    ) -> Result<(), GatekeeperError> {
        let path = format!("/community-voice-chat/{}", community_id);
        self.fire(
            reqwest::Method::DELETE,
            &path,
            Some(serde_json::json!({ "user_address": user_address })),
        )
        .await
    }

    /// POST/DELETE /community-voice-chat/:id/users/:addr/speak-request
    pub async fn request_to_speak(
        &self,
        community_id: &str,
        user_address: &str,
        raising_hand: bool,
    ) -> Result<(), GatekeeperError> {
        let path = format!(
            "/community-voice-chat/{}/users/{}/speak-request",
            community_id, user_address
        );
        let method = if raising_hand {
            reqwest::Method::POST
        } else {
            reqwest::Method::DELETE
        };
        self.fire(method, &path, None).await
    }

    /// DELETE /community-voice-chat/:id/users/:addr/speak-request (reject hand)
    pub async fn reject_speak_request(
        &self,
        community_id: &str,
        user_address: &str,
    ) -> Result<(), GatekeeperError> {
        let path = format!(
            "/community-voice-chat/{}/users/{}/speak-request",
            community_id, user_address
        );
        self.fire(reqwest::Method::DELETE, &path, None).await
    }

    /// POST/DELETE /community-voice-chat/:id/users/:addr/speaker
    pub async fn set_speaker(
        &self,
        community_id: &str,
        user_address: &str,
        promote: bool,
    ) -> Result<(), GatekeeperError> {
        let path = format!(
            "/community-voice-chat/{}/users/{}/speaker",
            community_id, user_address
        );
        let method = if promote {
            reqwest::Method::POST
        } else {
            reqwest::Method::DELETE
        };
        self.fire(method, &path, None).await
    }

    /// DELETE /community-voice-chat/:id/users/:addr (kick)
    pub async fn kick_player(
        &self,
        community_id: &str,
        user_address: &str,
    ) -> Result<(), GatekeeperError> {
        let path = format!(
            "/community-voice-chat/{}/users/{}",
            community_id, user_address
        );
        self.fire(reqwest::Method::DELETE, &path, None).await
    }

    /// PATCH /community-voice-chat/:id/users/:addr/mute
    pub async fn mute_speaker(
        &self,
        community_id: &str,
        user_address: &str,
        muted: bool,
    ) -> Result<(), GatekeeperError> {
        let path = format!(
            "/community-voice-chat/{}/users/{}/mute",
            community_id, user_address
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
                // 404 (no live room / participant) is treated as success: the
                // desired end-state (gone / not present) already holds.
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
