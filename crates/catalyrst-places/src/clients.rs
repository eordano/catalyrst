//! HTTP clients for the comms-gatekeeper and Events services, used by the
//! `/destinations` path to populate `connected_addresses` and `live` exactly as
//! upstream `entities/Destination/utils.ts` does.
//!
//! - `CommsGatekeeper` -> `GET /scene-participants?pointer=<base>&realm_name=main`
//!   (scenes) or `?realm_name=<world_name>` (worlds), returns `{ ok, data:
//!   { addresses: string[] } }`.
//! - `Events` -> `POST /events/search?list=live` with `{ placeIds: [...] }`,
//!   returns `{ ok, data: { events: [{ place_id, ... }], total } }`; a
//!   destination id is "live" iff it appears as some event's `place_id`.
//!
//! Both clients carry a 5-minute per-key in-memory TTL cache and a 10s request
//! timeout, mirroring the upstream API clients. On any error the upstream
//! returns empty (`[]` / `false`), and so do we.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct SceneParticipantsResponse {
    data: SceneParticipantsData,
}

#[derive(Debug, Deserialize)]
struct SceneParticipantsData {
    #[serde(default)]
    addresses: Vec<String>,
}

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

/// Client for comms-gatekeeper scene/world participant lists.
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
        cache.insert(
            key,
            Cached {
                value,
                expires_at: Instant::now() + CACHE_TTL,
            },
        );
    }

    async fn fetch_participants(&self, query: &[(&str, &str)]) -> Vec<String> {
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
                Ok(body) => body.data.addresses,
                Err(e) => {
                    tracing::debug!(error = %e, "scene-participants decode failed");
                    Vec::new()
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "scene-participants request failed");
                Vec::new()
            }
        }
    }

    /// `getSceneParticipants(pointer)` -> addresses connected to the scene room
    /// (realm defaults to "main"). Cache key `scene:<pointer>:<realm>`.
    pub async fn get_scene_participants(&self, pointer: &str) -> Vec<String> {
        let realm = "main";
        let key = format!("scene:{}:{}", pointer, realm);
        if let Some(v) = self.cache_get(&key) {
            return v;
        }
        let addrs = self
            .fetch_participants(&[("pointer", pointer), ("realm_name", realm)])
            .await;
        self.cache_put(key, addrs.clone());
        addrs
    }

    /// `getWorldParticipants(worldName)` -> addresses connected to the world
    /// room. The gatekeeper treats `realm_name` as the world name when no
    /// `pointer` is provided. Cache key `world:<worldName>`.
    pub async fn get_world_participants(&self, world_name: &str) -> Vec<String> {
        let key = format!("world:{}", world_name);
        if let Some(v) = self.cache_get(&key) {
            return v;
        }
        let addrs = self.fetch_participants(&[("realm_name", world_name)]).await;
        self.cache_put(key, addrs.clone());
        addrs
    }
}

/// Client for the Events live-status batch endpoint.
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

    /// `checkLiveEventsForDestinations(ids)` -> map id -> isLive. Land places
    /// pass the place UUID, worlds pass the world name. Caches each id for 5
    /// minutes; only uncached ids hit the network. Mirrors upstream: an id is
    /// live iff it is the `place_id` of a returned live event.
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

        // Default uncached to false, then mark live ones from the response.
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

        // Cache each uncached result.
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
