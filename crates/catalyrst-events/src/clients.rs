use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct SceneParticipantsResponse {
    #[serde(default)]
    data: SceneParticipantsData,
}

#[derive(Debug, Default, Deserialize)]
struct SceneParticipantsData {
    #[serde(default)]
    addresses: Vec<String>,
}

struct Cached<T> {
    value: T,
    expires_at: Instant,
}

pub struct CommsGatekeeper {
    base_url: String,
    http: reqwest::Client,
    cache: Mutex<HashMap<String, Cached<Vec<String>>>>,
}

impl CommsGatekeeper {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cache_get(&self, key: &str) -> Option<Vec<String>> {
        let cache = self.cache.lock().unwrap();
        cache
            .get(key)
            .filter(|c| c.expires_at > Instant::now())
            .map(|c| c.value.clone())
    }

    fn cache_put(&self, key: String, value: Vec<String>) {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        cache.retain(|_, c| c.expires_at > now);
        cache.insert(
            key,
            Cached {
                value,
                expires_at: now + CACHE_TTL,
            },
        );
    }

    async fn fetch_participants(&self, query: &[(&str, &str)]) -> Option<Vec<String>> {
        let url = format!("{}/scene-participants", self.base_url);
        let resp = self
            .http
            .get(&url)
            .query(query)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;
        match resp {
            Ok(r) => match r.json::<SceneParticipantsResponse>().await {
                Ok(body) => Some(body.data.addresses),
                Err(e) => {
                    tracing::debug!(error = %e, "scene-participants decode failed");
                    None
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "scene-participants request failed");
                None
            }
        }
    }

    pub async fn get_scene_participants(&self, pointer: &str) -> Vec<String> {
        let realm = "main";
        let key = format!("scene:{}:{}", pointer, realm);
        if let Some(v) = self.cache_get(&key) {
            return v;
        }
        match self
            .fetch_participants(&[("pointer", pointer), ("realm_name", realm)])
            .await
        {
            Some(addrs) => {
                self.cache_put(key, addrs.clone());
                addrs
            }
            None => Vec::new(),
        }
    }

    pub async fn get_world_participants(&self, world_name: &str) -> Vec<String> {
        let key = format!("world:{}", world_name);
        if let Some(v) = self.cache_get(&key) {
            return v;
        }
        match self.fetch_participants(&[("realm_name", world_name)]).await {
            Some(addrs) => {
                self.cache_put(key, addrs.clone());
                addrs
            }
            None => Vec::new(),
        }
    }
}
