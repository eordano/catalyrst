use serde::Deserialize;

#[derive(Clone)]
pub struct BansComponent {
    http: reqwest::Client,
    base_url: Option<String>,
    auth_token: Option<String>,
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
        let url = format!(
            "{}/worlds/{}/parcels/{}/users/{}/ban-status",
            base,
            urlencode(world_name),
            urlencode(scene_base_parcel),
            urlencode(address)
        );
        let resp = self.http.get(&url).bearer_auth(token).send().await;
        match resp {
            Ok(r) if r.status().is_success() => r
                .json::<SceneBanResponse>()
                .await
                .map(|b| b.is_banned)
                .unwrap_or(false),
            Ok(r) => {
                tracing::warn!(status = %r.status(), "comms-gatekeeper scene ban check non-2xx");
                false
            }
            Err(e) => {
                tracing::warn!(error = %e, "comms-gatekeeper scene ban check failed");
                false
            }
        }
    }

    pub async fn is_player_banned(&self, address: &str) -> bool {
        let (Some(base), Some(token)) = (self.base_url.as_ref(), self.auth_token.as_ref()) else {
            return false;
        };
        let url = format!("{}/users/{}/bans", base, address.to_lowercase());
        let resp = self.http.get(&url).bearer_auth(token).send().await;
        match resp {
            Ok(r) if r.status().is_success() => r
                .json::<PlatformBanResponse>()
                .await
                .ok()
                .and_then(|b| b.data)
                .map(|d| d.is_banned)
                .unwrap_or(false),
            Ok(r) => {
                tracing::warn!(status = %r.status(), "comms-gatekeeper platform ban check non-2xx");
                false
            }
            Err(e) => {
                tracing::warn!(error = %e, "comms-gatekeeper platform ban check failed");
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
