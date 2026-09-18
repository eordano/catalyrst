use std::time::Duration;

use moka::future::Cache;
use serde::Deserialize;

/// Definitive gatekeeper answers are remembered briefly; errors and non-2xx replies are
/// never memoized, so a transient failure keeps its per-request fallback.
const BAN_CACHE_TTL: Duration = Duration::from_secs(45);
const BAN_CACHE_CAPACITY: u64 = 50_000;

#[derive(Clone)]
pub struct BansComponent {
    http: reqwest::Client,
    base_url: Option<String>,
    auth_token: Option<String>,
    player_bans: Cache<String, bool>,
    scene_bans: Cache<(String, String, String), bool>,
}

#[derive(Debug, Deserialize)]
struct SceneBanResponse {
    #[serde(default)]
    #[serde(rename = "isBanned")]
    is_banned: bool,
}

#[derive(Debug, Deserialize)]
struct PlatformBanData {
    #[serde(default)]
    #[serde(rename = "isBanned")]
    is_banned: bool,
}

#[derive(Debug, Deserialize)]
struct PlatformBanResponse {
    #[serde(default)]
    data: Option<PlatformBanData>,
}

impl BansComponent {
    pub fn new(
        http: reqwest::Client,
        base_url: Option<String>,
        auth_token: Option<String>,
    ) -> Self {
        Self {
            http,
            base_url,
            auth_token,
            player_bans: Cache::builder()
                .time_to_live(BAN_CACHE_TTL)
                .max_capacity(BAN_CACHE_CAPACITY)
                .build(),
            scene_bans: Cache::builder()
                .time_to_live(BAN_CACHE_TTL)
                .max_capacity(BAN_CACHE_CAPACITY)
                .build(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.base_url.is_some() && self.auth_token.is_some()
    }

    pub async fn is_user_banned_from_scene(
        &self,
        address: &str,
        world_name: &str,
        scene_base_parcel: &str,
    ) -> bool {
        let (Some(base), Some(token)) = (self.base_url.as_ref(), self.auth_token.as_ref()) else {
            return false;
        };
        let key = (
            address.to_lowercase(),
            world_name.to_lowercase(),
            scene_base_parcel.to_string(),
        );
        if let Some(banned) = self.scene_bans.get(&key).await {
            return banned;
        }
        let url = format!(
            "{}/worlds/{}/parcels/{}/users/{}/ban-status",
            base,
            urlencode(world_name),
            urlencode(scene_base_parcel),
            urlencode(address)
        );
        let resp = self.http.get(&url).bearer_auth(token).send().await;
        match resp {
            Ok(r) if r.status().is_success() => match r.json::<SceneBanResponse>().await {
                Ok(b) => {
                    self.scene_bans.insert(key, b.is_banned).await;
                    b.is_banned
                }
                Err(_) => false,
            },
            Ok(r) => {
                tracing::warn!(status = %r.status(), "comms-gatekeeper scene ban check non-2xx");
                false
            }
            Err(_) => {
                tracing::warn!("comms-gatekeeper scene ban check failed");
                false
            }
        }
    }

    pub async fn is_player_banned(&self, address: &str) -> bool {
        let (Some(base), Some(token)) = (self.base_url.as_ref(), self.auth_token.as_ref()) else {
            return false;
        };
        let key = address.to_lowercase();
        if let Some(banned) = self.player_bans.get(&key).await {
            return banned;
        }
        let url = format!("{}/users/{}/bans", base, key);
        let resp = self.http.get(&url).bearer_auth(token).send().await;
        match resp {
            Ok(r) if r.status().is_success() => match r.json::<PlatformBanResponse>().await {
                Ok(b) => {
                    let banned = b.data.map(|d| d.is_banned).unwrap_or(false);
                    self.player_bans.insert(key, banned).await;
                    banned
                }
                Err(_) => false,
            },
            Ok(r) => {
                tracing::warn!(status = %r.status(), "comms-gatekeeper platform ban check non-2xx");
                false
            }
            Err(_) => {
                tracing::warn!("comms-gatekeeper platform ban check failed");
                false
            }
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[tokio::test(flavor = "current_thread")]
    async fn gatekeeper_transport_logs_never_retain_the_configured_url() {
        const CANARY: &str = "worlds-gatekeeper-private-material-canary";
        let bans = BansComponent::new(
            reqwest::Client::new(),
            Some(format!("http://127.0.0.1:1/{CANARY}")),
            Some("private-service-token".to_string()),
        );
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        assert!(
            !bans
                .is_user_banned_from_scene("0xabc", "world", "0,0")
                .await
        );
        assert!(!bans.is_player_banned("0xabc").await);

        let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("comms-gatekeeper scene ban check failed"));
        assert!(logs.contains("comms-gatekeeper platform ban check failed"));
        assert!(!logs.contains(CANARY), "configured URL survived in {logs}");
    }
}
