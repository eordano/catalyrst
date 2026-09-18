use std::time::Duration;

use chrono::Utc;
use serde::Serialize;

use crate::config::LivekitConfig;

const REMOVE_PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(2);
pub const MAX_TOKEN_TTL_SECS: i64 = 60;

#[derive(Clone, Debug)]
pub struct LivekitMinter {
    cfg: LivekitConfig,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct VideoGrant {
    room: String,
    #[serde(rename = "roomJoin")]
    room_join: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    sub: &'a str,
    nbf: i64,
    exp: i64,
    iat: i64,
    jti: String,
    name: &'a str,
    video: VideoGrant,
}

#[derive(Clone, Debug, Serialize)]
pub struct LivekitGrant {
    pub url: String,
    pub room: String,
    pub identity: String,
    pub token: Option<String>,
    pub expires_at: i64,
}

impl LivekitMinter {
    pub fn new(cfg: LivekitConfig) -> Self {
        Self {
            cfg,
            http: reqwest::Client::new(),
        }
    }

    pub fn ws_url(&self) -> &str {
        &self.cfg.ws_url
    }

    pub async fn remove_participant(&self, room: &str, identity: &str) {
        let (Some(key), Some(secret)) =
            (self.cfg.api_key.as_deref(), self.cfg.api_secret.as_deref())
        else {
            return;
        };
        let token = match catalyrst_livekit::room_admin_token(key, secret, room) {
            Ok(t) => t,
            Err(_) => {
                tracing::warn!(
                    room,
                    identity,
                    reason = "token mint failed",
                    "livekit remove_participant: admin token mint failed"
                );
                return;
            }
        };
        let url = format!(
            "{}/twirp/livekit.RoomService/RemoveParticipant",
            catalyrst_livekit::api_base_url(&self.cfg.ws_url)
        );
        let body = serde_json::json!({ "room": room, "identity": identity });
        match self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .timeout(REMOVE_PARTICIPANT_TIMEOUT)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                tracing::info!(
                    room,
                    identity,
                    "livekit remove_participant: evicted from SFU"
                );
            }
            Ok(r) => {
                tracing::warn!(room, identity, status = %r.status(), "livekit remove_participant: non-OK status");
            }
            Err(_) => {
                tracing::warn!(room, identity, "livekit remove_participant: request failed");
            }
        }
    }

    pub fn is_armed(&self) -> bool {
        self.cfg.api_key.is_some() && self.cfg.api_secret.is_some()
    }

    fn token_lifetime_secs(&self) -> i64 {
        match self.cfg.token_ttl_secs {
            configured if configured > 0 => configured.min(MAX_TOKEN_TTL_SECS),
            _ => MAX_TOKEN_TTL_SECS,
        }
    }

    pub fn mint(&self, identity: &str, room: &str) -> LivekitGrant {
        let now = Utc::now().timestamp();
        let exp = now + self.token_lifetime_secs();
        let token = match (self.cfg.api_key.as_deref(), self.cfg.api_secret.as_deref()) {
            (Some(key), Some(secret)) => Some(self.sign_jwt(key, secret, identity, room, now, exp)),
            _ => None,
        };
        LivekitGrant {
            url: self.cfg.ws_url.clone(),
            room: room.to_string(),
            identity: identity.to_string(),
            token,
            expires_at: exp,
        }
    }

    fn sign_jwt(
        &self,
        api_key: &str,
        api_secret: &str,
        identity: &str,
        room: &str,
        iat: i64,
        exp: i64,
    ) -> String {
        let claims = Claims {
            iss: api_key,
            sub: identity,
            nbf: iat,
            exp,
            iat,
            jti: uuid::Uuid::new_v4().to_string(),
            name: identity,
            video: VideoGrant {
                room: room.to_string(),
                room_join: true,
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
            },
        };
        let header = serde_json::json!({ "alg": "HS256", "typ": "JWT" });
        let header_json = serde_json::to_vec(&header).expect("header json");
        let claims_json = serde_json::to_vec(&claims).expect("claims json");
        catalyrst_livekit::sign_hs256(api_secret, &header_json, &claims_json)
            .expect("HMAC accepts any key length")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for LogBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn jwt_has_three_parts() {
        let m = LivekitMinter::new(LivekitConfig {
            api_key: Some("APIabc".into()),
            api_secret: Some("supersecret".into()),
            ws_url: "wss://lk.example".into(),
            token_ttl_secs: 60,
            comms_gatekeeper_url: None,
        });
        let g = m.mint("0xpeer", "I7");
        let tok = g.token.expect("armed minter mints");
        assert_eq!(tok.split('.').count(), 3);
        assert_eq!(g.url, "wss://lk.example");
        assert_eq!(g.room, "I7");
    }

    #[test]
    fn a_configured_lifetime_never_outlives_the_takeover_quarantine() {
        for configured in [21_600, 61, 0, -5] {
            let minter = LivekitMinter::new(LivekitConfig {
                api_key: Some("APIabc".into()),
                api_secret: Some("supersecret".into()),
                ws_url: "wss://lk.example".into(),
                token_ttl_secs: configured,
                comms_gatekeeper_url: None,
            });
            let before = Utc::now().timestamp();
            let grant = minter.mint("0xpeer", "I7");
            assert!(grant.expires_at > before);
            assert!(grant.expires_at <= Utc::now().timestamp() + MAX_TOKEN_TTL_SECS);
            let token = grant.token.expect("armed minter mints");
            let claims: serde_json::Value = serde_json::from_slice(
                &URL_SAFE_NO_PAD
                    .decode(token.split('.').nth(1).expect("claims part"))
                    .expect("base64 claims"),
            )
            .expect("claims json");
            assert_eq!(claims["exp"].as_i64(), Some(grant.expires_at));
        }
        let shorter = LivekitMinter::new(LivekitConfig {
            token_ttl_secs: 20,
            ..LivekitConfig::default()
        });
        assert!(shorter.mint("0xpeer", "I7").expires_at <= Utc::now().timestamp() + 20);
    }

    #[test]
    fn unarmed_minter_returns_no_token() {
        let m = LivekitMinter::new(LivekitConfig::default());
        let g = m.mint("0xpeer", "I7");
        assert!(g.token.is_none());
    }

    #[test]
    fn jwt_signature_matches_recomputed_hmac() {
        let key = "APIabc";
        let secret = "supersecret";
        let m = LivekitMinter::new(LivekitConfig {
            api_key: Some(key.into()),
            api_secret: Some(secret.into()),
            ws_url: "wss://lk.example".into(),
            token_ttl_secs: 60,
            comms_gatekeeper_url: None,
        });
        let g = m.mint("0xpeer", "I7");
        let tok = g.token.unwrap();
        let parts: Vec<&str> = tok.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signing_input.as_bytes());
        let want = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        assert_eq!(parts[2], want);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn participant_removal_logs_never_retain_the_configured_url() {
        const CANARY: &str = "archipelago-livekit-private-material-canary";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_url = format!("ws://{}/{CANARY}", listener.local_addr().unwrap());
        drop(listener);
        let minter = LivekitMinter::new(LivekitConfig {
            api_key: Some("APIabc".into()),
            api_secret: Some("supersecret".into()),
            ws_url,
            token_ttl_secs: 60,
            comms_gatekeeper_url: None,
        });
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        minter.remove_participant("room", "identity").await;

        let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("livekit remove_participant: request failed"));
        assert!(!logs.contains(CANARY), "configured URL survived in {logs}");
    }
}
