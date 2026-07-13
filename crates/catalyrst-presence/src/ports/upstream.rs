use std::time::Duration;

use serde_json::Value;
use url::form_urlencoded::byte_serialize;

use crate::config::Config;

const USER_AGENT: &str = concat!("catalyrst-presence/", env!("CARGO_PKG_VERSION"));
const MAX_ATTEMPTS: u32 = 6;

#[derive(Clone)]
pub struct UpstreamClient {
    http: reqwest::Client,
    archipelago_url: String,
    comms_url: String,
    worlds_server_url: String,
    genesis_realm: String,
}

impl UpstreamClient {
    pub fn new(cfg: &Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            archipelago_url: cfg.archipelago_url.clone(),
            comms_url: cfg.comms_url.clone(),
            worlds_server_url: cfg.worlds_server_url.clone(),
            genesis_realm: cfg.genesis_realm.clone(),
        })
    }

    pub fn genesis_realm(&self) -> &str {
        &self.genesis_realm
    }

    async fn get_json(&self, url: &str) -> anyhow::Result<Option<Value>> {
        let mut backoff = Duration::from_secs(2);
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.as_u16() == 404 || status.as_u16() == 204 {
                        return Ok(None);
                    }
                    if status.as_u16() == 429 || status.is_server_error() {
                        tracing::warn!(%url, code = status.as_u16(), "retrying after error");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }
                    if !status.is_success() {
                        anyhow::bail!("HTTP {} on {}", status.as_u16(), url);
                    }
                    let body = resp.bytes().await?;
                    if body.is_empty() {
                        return Ok(None);
                    }
                    let value: Value = serde_json::from_slice(&body)?;
                    return Ok(Some(value));
                }
                Err(e) => {
                    if attempt + 1 == MAX_ATTEMPTS {
                        return Err(e.into());
                    }
                    tracing::warn!(%url, error = %e, "network error; backing off");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
        tracing::error!(%url, "exhausted retries");
        Ok(None)
    }

    pub async fn peers(&self) -> anyhow::Result<Value> {
        Ok(self
            .get_json(&format!("{}/peers", self.archipelago_url))
            .await?
            .unwrap_or_else(|| Value::Array(vec![])))
    }

    pub async fn islands(&self) -> anyhow::Result<Value> {
        Ok(self
            .get_json(&format!("{}/islands", self.archipelago_url))
            .await?
            .unwrap_or_else(|| Value::Array(vec![])))
    }

    pub async fn hot_scenes(&self) -> anyhow::Result<Value> {
        Ok(self
            .get_json(&format!("{}/hot-scenes", self.archipelago_url))
            .await?
            .unwrap_or_else(|| Value::Array(vec![])))
    }

    pub async fn scene_participants(&self, pointer: &str) -> anyhow::Result<Option<Value>> {
        let url = format!(
            "{}/scene-participants?pointer={}&realm_name={}",
            self.comms_url,
            encode(pointer),
            encode(&self.genesis_realm),
        );
        self.get_json(&url).await
    }

    pub async fn world_participants(&self, world: &str) -> anyhow::Result<Option<Value>> {
        let url = format!(
            "{}/scene-participants?realm_name={}",
            self.comms_url,
            encode(world),
        );
        self.get_json(&url).await
    }

    pub async fn worlds_live_data(&self) -> anyhow::Result<Option<Value>> {
        let url = format!("{}/live-data", self.worlds_server_url);
        self.get_json(&url).await
    }
}

fn encode(s: &str) -> String {
    byte_serialize(s.as_bytes()).collect()
}
