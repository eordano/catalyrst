use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

use crate::config::LivekitConfig;

#[derive(Clone, Debug)]
pub struct LivekitMinter {
    cfg: LivekitConfig,
}

#[derive(Serialize)]
struct Header<'a> {
    alg: &'a str,
    typ: &'a str,
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
        Self { cfg }
    }

    pub fn ws_url(&self) -> &str {
        &self.cfg.ws_url
    }

    pub fn is_armed(&self) -> bool {
        self.cfg.api_key.is_some() && self.cfg.api_secret.is_some()
    }

    pub fn mint(&self, identity: &str, room: &str) -> LivekitGrant {
        let now = Utc::now().timestamp();
        let exp = now + self.cfg.token_ttl_secs;
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
        let header = Header {
            alg: "HS256",
            typ: "JWT",
        };
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
        let header_json = serde_json::to_vec(&header).expect("header json");
        let claims_json = serde_json::to_vec(&claims).expect("claims json");
        let header_b64 = URL_SAFE_NO_PAD.encode(header_json);
        let claims_b64 = URL_SAFE_NO_PAD.encode(claims_json);
        let signing_input = format!("{}.{}", header_b64, claims_b64);
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(api_secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(signing_input.as_bytes());
        let sig = mac.finalize().into_bytes();
        let sig_b64 = URL_SAFE_NO_PAD.encode(sig);
        format!("{}.{}", signing_input, sig_b64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_has_three_parts() {
        let m = LivekitMinter::new(LivekitConfig {
            api_key: Some("APIabc".into()),
            api_secret: Some("supersecret".into()),
            ws_url: "wss://lk.example".into(),
            token_ttl_secs: 60,
        });
        let g = m.mint("0xpeer", "I7");
        let tok = g.token.expect("armed minter mints");
        assert_eq!(tok.split('.').count(), 3);
        assert_eq!(g.url, "wss://lk.example");
        assert_eq!(g.room, "I7");
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
        });
        let g = m.mint("0xpeer", "I7");
        let tok = g.token.unwrap();
        let parts: Vec<&str> = tok.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signing_input.as_bytes());
        let want = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        assert_eq!(parts[2], want);
    }
}
