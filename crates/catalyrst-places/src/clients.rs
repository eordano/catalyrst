use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

pub use catalyrst_fed::comms::CommsGatekeeper;

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct EventsResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    data: Option<EventsData>,
}

#[derive(Debug, Deserialize)]
struct EventsData {
    #[serde(default)]
    events: Vec<EventEntry>,
}

#[derive(Debug, Deserialize)]
struct EventEntry {
    #[serde(default)]
    place_id: Option<String>,
}

struct Cached<T> {
    value: T,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct CurrentScenesResponse {
    #[serde(default)]
    scenes: Vec<PresenceSceneRow>,
}

#[derive(Debug, Deserialize)]
struct PresenceSceneRow {
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    count: i32,
}

#[derive(Debug, Deserialize)]
struct CurrentWorldsResponse {
    #[serde(default)]
    worlds: Vec<PresenceWorldRow>,
}

#[derive(Debug, Deserialize)]
struct PresenceWorldRow {
    #[serde(default)]
    world_name: String,
    #[serde(default)]
    count: i32,
    #[serde(default)]
    live_users: Option<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct LiveUserCounts {
    pub places: Vec<(String, i32)>,
    pub worlds: Vec<(String, i32)>,
}

const PRESENCE_CACHE_TTL: Duration = Duration::from_secs(30);

pub struct Presence {
    base_url: String,
    http: reqwest::Client,
    cache: Mutex<Option<Cached<LiveUserCounts>>>,
}

impl Presence {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            cache: Mutex::new(None),
        }
    }

    fn cache_get(&self) -> Option<LiveUserCounts> {
        let cache = self.cache.lock().unwrap();
        cache
            .as_ref()
            .filter(|c| c.expires_at > Instant::now())
            .map(|c| c.value.clone())
    }

    fn cache_put(&self, value: LiveUserCounts) {
        let mut cache = self.cache.lock().unwrap();
        *cache = Some(Cached {
            value,
            expires_at: Instant::now() + PRESENCE_CACHE_TTL,
        });
    }

    async fn fetch_scenes(&self) -> Vec<(String, i32)> {
        let url = format!("{}/current/scenes", self.base_url);
        let resp = self.http.get(&url).timeout(REQUEST_TIMEOUT).send().await;
        match resp {
            Ok(r) => match r.json::<CurrentScenesResponse>().await {
                Ok(body) => body
                    .scenes
                    .into_iter()
                    .filter(|s| !s.pointer.is_empty())
                    .map(|s| (s.pointer, s.count))
                    .collect(),
                Err(e) => {
                    tracing::debug!(error = %e, "presence current/scenes decode failed");
                    Vec::new()
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "presence current/scenes request failed");
                Vec::new()
            }
        }
    }

    async fn fetch_worlds(&self) -> Vec<(String, i32)> {
        let url = format!("{}/current/worlds", self.base_url);
        let resp = self.http.get(&url).timeout(REQUEST_TIMEOUT).send().await;
        match resp {
            Ok(r) => match r.json::<CurrentWorldsResponse>().await {
                Ok(body) => body
                    .worlds
                    .into_iter()
                    .filter(|w| !w.world_name.is_empty())
                    .map(|w| (w.world_name, w.live_users.unwrap_or(w.count)))
                    .collect(),
                Err(e) => {
                    tracing::debug!(error = %e, "presence current/worlds decode failed");
                    Vec::new()
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "presence current/worlds request failed");
                Vec::new()
            }
        }
    }

    pub async fn live_user_counts(&self) -> LiveUserCounts {
        if let Some(v) = self.cache_get() {
            return v;
        }
        let (places, worlds) = tokio::join!(self.fetch_scenes(), self.fetch_worlds());
        let counts = LiveUserCounts { places, worlds };
        self.cache_put(counts.clone());
        counts
    }
}

pub struct Events {
    base_url: String,
    http: reqwest::Client,
    cache: Mutex<HashMap<String, Cached<bool>>>,
}

impl Events {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub async fn check_live_events(&self, ids: &[String]) -> HashMap<String, bool> {
        let mut out: HashMap<String, bool> = HashMap::new();
        if ids.is_empty() {
            return out;
        }

        let now = Instant::now();
        let mut uncached: Vec<String> = Vec::new();
        {
            let cache = self.cache.lock().unwrap();
            for id in ids {
                match cache.get(id) {
                    Some(c) if c.expires_at > now => {
                        out.insert(id.clone(), c.value);
                    }
                    _ => uncached.push(id.clone()),
                }
            }
        }
        if uncached.is_empty() {
            return out;
        }

        for id in &uncached {
            out.insert(id.clone(), false);
        }

        let url = format!("{}/events/search?list=live", self.base_url);
        let body = serde_json::json!({ "placeIds": uncached });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;

        match resp {
            Ok(r) => match r.json::<EventsResponse>().await {
                Ok(parsed) => {
                    if parsed.ok {
                        if let Some(data) = parsed.data {
                            for ev in data.events {
                                if let Some(pid) = ev.place_id {
                                    if uncached.contains(&pid) {
                                        out.insert(pid, true);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => tracing::debug!(error = %e, "events search decode failed"),
            },
            Err(e) => tracing::debug!(error = %e, "events search request failed"),
        }

        {
            let mut cache = self.cache.lock().unwrap();
            let expires_at = Instant::now() + CACHE_TTL;
            for id in &uncached {
                let is_live = *out.get(id).unwrap_or(&false);
                cache.insert(
                    id.clone(),
                    Cached {
                        value: is_live,
                        expires_at,
                    },
                );
            }
        }

        out
    }
}
