use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use serde_json::Value;

use catalyrst_fed::cache::{cache_get, cache_put, Cached};
pub use catalyrst_fed::comms::CommsGatekeeper;

use crate::http::errors::ApiError;

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
// Upstream refreshes its next-event snapshot every 20s; the same horizon
// keeps a burst of decorated reads from re-asking the events service.
const NEXT_EVENT_CACHE_TTL: Duration = Duration::from_secs(20);
// Upstream answers next events with one DISTINCT ON per destination; the
// search here is a global soonest-first page, so a full page keeps paging
// until every asked destination has its hit, bounded so one busy catalogue
// cannot turn a decoration into a scan.
const NEXT_EVENT_PAGE: usize = 500;
const NEXT_EVENT_MAX_PAGES: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// The base is the events service root whether the deployment names it bare
// or with its legacy `/api` suffix: every route here is spelled from the
// root, so the suffix would double up.
fn service_root(base_url: &str) -> String {
    base_url
        .trim_end_matches('/')
        .trim_end_matches("/api")
        .trim_end_matches('/')
        .to_string()
}

// The id is caller input that becomes one path segment of an internal
// request, so it must never be able to add segments, a query or a fragment.
pub(crate) fn destination_events_url(base_url: &str, id: &str) -> String {
    format!(
        "{}/v1/destinations/{}/events",
        base_url,
        utf8_percent_encode(id, NON_ALPHANUMERIC)
    )
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
    id: Option<String>,
    #[serde(default)]
    place_id: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    world: bool,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    next_start_at: Option<String>,
}

// A search is asked by destination key: a place by its id and a world by its
// name, the way upstream keys its live/next snapshots. A hit names its
// destination by the wire `place_id` or, for a world event, by the world in
// `server`, and either resolves case-insensitively to the key asked with.
struct AskedKeys(HashMap<String, String>);

impl AskedKeys {
    fn new(asked: &[String]) -> Self {
        Self(
            asked
                .iter()
                .map(|key| (key.to_lowercase(), key.clone()))
                .collect(),
        )
    }

    fn resolve(&self, ev: &EventEntry) -> Option<String> {
        let by_place = ev
            .place_id
            .as_deref()
            .and_then(|id| self.0.get(&id.to_lowercase()));
        let by_world = ev
            .server
            .as_deref()
            .filter(|_| ev.world)
            .and_then(|name| self.0.get(&name.to_lowercase()));
        by_place.or(by_world).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextEvent {
    pub id: String,
    pub name: String,
    pub next_start_at: String,
}

// The events service answers a `list=upcoming` search soonest-first, so the
// first hit per destination is its next event.
fn record_next_event(
    out: &mut HashMap<String, Option<NextEvent>>,
    asked: &AskedKeys,
    ev: EventEntry,
) {
    let Some(key) = asked.resolve(&ev) else {
        return;
    };
    let (Some(id), Some(name), Some(next_start_at)) = (ev.id, ev.name, ev.next_start_at) else {
        return;
    };
    let Some(slot) = out.get_mut(&key) else {
        return;
    };
    if slot.is_none() {
        *slot = Some(NextEvent {
            id,
            name,
            next_start_at,
        });
    }
}

fn has_unresolved(out: &HashMap<String, Option<NextEvent>>, asked: &[String]) -> bool {
    asked
        .iter()
        .any(|id| out.get(id).is_none_or(Option::is_none))
}

#[derive(Debug, Deserialize)]
struct CategoriesResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    data: Option<Vec<Value>>,
}

// Only the caller's identity travels to the events service: its auth chain,
// the admin bearer, and the proxy's original path so the signature still
// binds to the route the caller signed.
fn forwarded_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for (name, value) in headers.iter() {
        let name = name.as_str();
        if !(name.starts_with("x-identity-")
            || name == "authorization"
            || name == "x-original-path")
        {
            continue;
        }
        let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) else {
            continue;
        };
        out.append(name, value);
    }
    out
}

fn record_live_event(out: &mut HashMap<String, Option<String>>, asked: &AskedKeys, ev: EventEntry) {
    let Some(key) = asked.resolve(&ev) else {
        return;
    };
    if out.get(&key).is_some_and(Option::is_some) {
        return;
    }
    out.insert(key, ev.name.filter(|n| !n.is_empty()));
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
    cache: Mutex<HashMap<String, Cached<Option<String>>>>,
    next_cache: Mutex<HashMap<String, Cached<Option<NextEvent>>>>,
}

impl Events {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: service_root(&base_url),
            http: reqwest::Client::new(),
            cache: Mutex::new(HashMap::new()),
            next_cache: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn url(&self, suffix: &str) -> String {
        format!("{}{}", self.base_url, suffix)
    }

    async fn fetch_upcoming(&self, ids: &[String], offset: usize) -> Option<Vec<EventEntry>> {
        let url = self.url(&format!(
            "/api/events/search?list=upcoming&limit={NEXT_EVENT_PAGE}&offset={offset}"
        ));
        let body = serde_json::json!({ "placeIds": ids });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;
        match resp {
            Ok(r) => match r.json::<EventsResponse>().await {
                Ok(parsed) if parsed.ok => Some(parsed.data.map(|d| d.events).unwrap_or_default()),
                Ok(_) => None,
                Err(e) => {
                    tracing::debug!(error = %e, "events upcoming search decode failed");
                    None
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "events upcoming search request failed");
                None
            }
        }
    }

    pub async fn next_events(&self, ids: &[String]) -> HashMap<String, NextEvent> {
        let mut out: HashMap<String, Option<NextEvent>> = HashMap::new();
        let mut uncached: Vec<String> = Vec::new();
        for id in ids {
            match cache_get(&self.next_cache, id) {
                Some(value) => {
                    out.insert(id.clone(), value);
                }
                None => {
                    out.insert(id.clone(), None);
                    uncached.push(id.clone());
                }
            }
        }
        if !uncached.is_empty() {
            let asked = AskedKeys::new(&uncached);
            let mut fetched = true;
            let mut offset = 0;
            for _ in 0..NEXT_EVENT_MAX_PAGES {
                let Some(page) = self.fetch_upcoming(&uncached, offset).await else {
                    fetched = false;
                    break;
                };
                let page_len = page.len();
                for ev in page {
                    record_next_event(&mut out, &asked, ev);
                }
                if page_len < NEXT_EVENT_PAGE || !has_unresolved(&out, &uncached) {
                    break;
                }
                offset += page_len;
            }
            // A failed search says nothing about a destination, so only a
            // completed one may cache the absence of a next event.
            if fetched {
                for id in &uncached {
                    let value = out.get(id).cloned().unwrap_or(None);
                    cache_put(&self.next_cache, id.clone(), value, NEXT_EVENT_CACHE_TTL);
                }
            }
        }
        out.into_iter()
            .filter_map(|(id, next)| next.map(|n| (id, n)))
            .collect()
    }

    #[cfg(test)]
    fn cached_next_event(&self, id: &str) -> Option<Option<NextEvent>> {
        cache_get(&self.next_cache, id)
    }

    #[cfg(test)]
    fn cached_live_event(&self, id: &str) -> Option<Option<String>> {
        cache_get(&self.cache, id)
    }

    pub async fn event_categories(&self) -> Result<Vec<Value>, ApiError> {
        let url = self.url("/api/events/categories");
        let resp = self
            .http
            .get(&url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "events categories request failed");
                ApiError::service_unavailable("events service unavailable")
            })?;
        let parsed = resp.json::<CategoriesResponse>().await.map_err(|e| {
            tracing::warn!(error = %e, "events categories decode failed");
            ApiError::service_unavailable("events service unavailable")
        })?;
        match (parsed.ok, parsed.data) {
            (true, Some(data)) => Ok(data),
            _ => Err(ApiError::service_unavailable("events service unavailable")),
        }
    }

    pub async fn destination_events(
        &self,
        id: &str,
        headers: &HeaderMap,
    ) -> Result<(u16, Value), ApiError> {
        let url = destination_events_url(&self.base_url, id);
        let resp = self
            .http
            .get(&url)
            .headers(forwarded_headers(headers))
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "events destination request failed");
                ApiError::service_unavailable("events service unavailable")
            })?;
        let status = resp.status().as_u16();
        let body = resp.json::<Value>().await.map_err(|e| {
            tracing::warn!(error = %e, "events destination decode failed");
            ApiError::service_unavailable("events service unavailable")
        })?;
        Ok((status, body))
    }

    pub async fn check_live_events(&self, ids: &[String]) -> HashMap<String, Option<String>> {
        let mut out: HashMap<String, Option<String>> = HashMap::new();
        if ids.is_empty() {
            return out;
        }

        let mut uncached: Vec<String> = Vec::new();
        for id in ids {
            match cache_get(&self.cache, id) {
                Some(value) => {
                    out.insert(id.clone(), value);
                }
                None => uncached.push(id.clone()),
            }
        }
        if uncached.is_empty() {
            return out;
        }

        for id in &uncached {
            out.insert(id.clone(), None);
        }

        // A failed search says nothing about a destination, so only a
        // completed one may cache the absence of a live event.
        let Some(events) = self.fetch_live(&uncached).await else {
            return out;
        };
        let asked = AskedKeys::new(&uncached);
        for ev in events {
            record_live_event(&mut out, &asked, ev);
        }
        for id in &uncached {
            let event_name = out.get(id).cloned().unwrap_or(None);
            cache_put(&self.cache, id.clone(), event_name, CACHE_TTL);
        }

        out
    }

    async fn fetch_live(&self, ids: &[String]) -> Option<Vec<EventEntry>> {
        let url = self.url("/api/events/search?list=live");
        let body = serde_json::json!({ "placeIds": ids });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;
        match resp {
            Ok(r) => match r.json::<EventsResponse>().await {
                Ok(parsed) if parsed.ok => Some(parsed.data.map(|d| d.events).unwrap_or_default()),
                Ok(_) => None,
                Err(e) => {
                    tracing::debug!(error = %e, "events search decode failed");
                    None
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "events search request failed");
                None
            }
        }
    }
}

#[cfg(test)]
mod live_event_tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn event(place_id: Option<&str>, name: Option<&str>) -> EventEntry {
        EventEntry {
            id: None,
            place_id: place_id.map(str::to_string),
            server: None,
            world: false,
            name: name.map(str::to_string),
            next_start_at: None,
        }
    }

    fn hosted(place_id: Option<&str>, server: &str, world: bool, name: &str) -> EventEntry {
        EventEntry {
            server: Some(server.to_string()),
            world,
            ..event(place_id, Some(name))
        }
    }

    fn search(uncached: &[String], events: Vec<EventEntry>) -> HashMap<String, Option<String>> {
        let mut out = HashMap::new();
        for id in uncached {
            out.insert(id.clone(), None);
        }
        let asked = AskedKeys::new(uncached);
        for ev in events {
            record_live_event(&mut out, &asked, ev);
        }
        out
    }

    #[test]
    fn a_world_event_resolves_by_its_world_whatever_the_wire_place_id() {
        let uncached = ids(&["My-World.dcl.eth", "place-1"]);
        for place_id in [Some("upstream-uuid"), None] {
            let out = search(
                &uncached,
                vec![hosted(place_id, "my-world.dcl.eth", true, "World Party")],
            );
            assert_eq!(out["My-World.dcl.eth"].as_deref(), Some("World Party"));
            assert_eq!(out["place-1"], None);
        }
    }

    #[test]
    fn a_scene_event_never_resolves_by_its_realm() {
        let uncached = ids(&["peer.decentraland.org", "Place-1"]);
        let out = search(
            &uncached,
            vec![hosted(
                Some("place-1"),
                "peer.decentraland.org",
                false,
                "Movie Night",
            )],
        );
        assert_eq!(out["peer.decentraland.org"], None);
        assert_eq!(
            out["Place-1"].as_deref(),
            Some("Movie Night"),
            "the wire place_id resolves whatever the case asked with"
        );
    }

    #[test]
    fn names_the_event_of_a_place_and_a_world_and_nulls_the_rest() {
        let uncached = ids(&["place-1", "place-2", "my-world.dcl.eth"]);
        let out = search(
            &uncached,
            vec![
                event(Some("place-1"), Some("Movie Night")),
                event(Some("my-world.dcl.eth"), Some("World Party")),
            ],
        );
        assert_eq!(out["place-1"].as_deref(), Some("Movie Night"));
        assert_eq!(out["my-world.dcl.eth"].as_deref(), Some("World Party"));
        assert_eq!(out["place-2"], None);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn first_event_wins_for_a_destination_hosting_several() {
        let uncached = ids(&["place-1"]);
        let out = search(
            &uncached,
            vec![
                event(Some("place-1"), Some("Movie Night")),
                event(Some("place-1"), Some("Late Set")),
            ],
        );
        assert_eq!(out["place-1"].as_deref(), Some("Movie Night"));
    }

    #[test]
    fn an_unnamed_hit_yields_to_a_later_named_one() {
        let uncached = ids(&["place-1"]);
        let out = search(
            &uncached,
            vec![
                event(Some("place-1"), None),
                event(Some("place-1"), Some("Late Set")),
            ],
        );
        assert_eq!(out["place-1"].as_deref(), Some("Late Set"));
    }

    #[test]
    fn events_for_ids_not_asked_about_are_ignored() {
        let uncached = ids(&["place-1"]);
        let out = search(
            &uncached,
            vec![
                event(Some("place-9"), Some("Elsewhere")),
                event(None, Some("Nowhere")),
            ],
        );
        assert_eq!(out["place-1"], None);
        assert_eq!(out.len(), 1);
    }
}

#[cfg(test)]
mod next_event_tests {
    use super::*;

    fn upcoming(place_id: &str, id: &str, name: &str, at: &str) -> EventEntry {
        EventEntry {
            id: Some(id.to_string()),
            place_id: Some(place_id.to_string()),
            server: None,
            world: false,
            name: Some(name.to_string()),
            next_start_at: Some(at.to_string()),
        }
    }

    fn asked(ids: &[&str]) -> (HashMap<String, Option<NextEvent>>, AskedKeys) {
        let keys = AskedKeys::new(&self::ids(ids));
        (ids.iter().map(|id| (id.to_string(), None)).collect(), keys)
    }

    #[test]
    fn the_first_upcoming_hit_per_destination_is_its_next_event() {
        let (mut out, asked) = asked(&["place-1", "my-world.dcl.eth", "place-2"]);
        record_next_event(
            &mut out,
            &asked,
            upcoming("place-1", "ev-a", "Soon", "2030-01-01T10:00:00Z"),
        );
        record_next_event(
            &mut out,
            &asked,
            upcoming("place-1", "ev-b", "Later", "2030-01-02T10:00:00Z"),
        );
        record_next_event(
            &mut out,
            &asked,
            upcoming("my-world.dcl.eth", "ev-c", "Party", "2030-01-03T10:00:00Z"),
        );
        assert_eq!(out["place-1"].as_ref().map(|n| n.id.as_str()), Some("ev-a"));
        assert_eq!(
            out["my-world.dcl.eth"].as_ref().map(|n| n.name.as_str()),
            Some("Party")
        );
        assert_eq!(out["place-2"], None);
    }

    #[test]
    fn a_world_is_keyed_by_its_name_even_when_the_wire_place_id_is_upstreams() {
        let (mut out, asked) = asked(&["My-World.dcl.eth"]);
        record_next_event(
            &mut out,
            &asked,
            EventEntry {
                server: Some("my-world.dcl.eth".to_string()),
                world: true,
                ..upcoming("upstream-uuid", "ev-c", "Party", "2030-01-03T10:00:00Z")
            },
        );
        assert_eq!(
            out["My-World.dcl.eth"].as_ref().map(|n| n.id.as_str()),
            Some("ev-c")
        );
    }

    #[test]
    fn hits_for_other_destinations_and_incomplete_rows_are_ignored() {
        let (mut out, asked) = asked(&["place-1"]);
        record_next_event(
            &mut out,
            &asked,
            upcoming("place-9", "ev-x", "Elsewhere", "2030-01-01T10:00:00Z"),
        );
        record_next_event(
            &mut out,
            &asked,
            EventEntry {
                id: None,
                ..upcoming("place-1", "ev-y", "Unnamed", "2030-01-01T10:00:00Z")
            },
        );
        assert_eq!(out["place-1"], None);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_destination_stays_unresolved_until_it_has_a_hit() {
        let uncached = ids(&["place-1", "place-2"]);
        let (mut out, asked) = asked(&["place-1", "place-2"]);
        assert!(has_unresolved(&out, &uncached));
        record_next_event(
            &mut out,
            &asked,
            upcoming("place-1", "ev-a", "Soon", "2030-01-01T10:00:00Z"),
        );
        assert!(has_unresolved(&out, &uncached));
        record_next_event(
            &mut out,
            &asked,
            upcoming("place-2", "ev-b", "Later", "2030-01-02T10:00:00Z"),
        );
        assert!(!has_unresolved(&out, &uncached));
    }

    #[tokio::test]
    async fn a_failed_search_caches_no_absence() {
        let events = Events::new("http://127.0.0.1:9".into());
        let next = events.next_events(&ids(&["place-1"])).await;
        assert!(next.is_empty());
        assert_eq!(
            events.cached_next_event("place-1"),
            None,
            "an unreachable events service must not suppress the next retry"
        );
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_identity_headers_are_forwarded_to_the_events_service() {
        let mut headers = HeaderMap::new();
        headers.insert("x-identity-auth-chain-0", "{}".parse().unwrap());
        headers.insert("x-identity-timestamp", "1".parse().unwrap());
        headers.insert("authorization", "Bearer t".parse().unwrap());
        headers.insert(
            "x-original-path",
            "/v1/destinations/x/events".parse().unwrap(),
        );
        headers.insert("cookie", "secret=1".parse().unwrap());
        headers.insert("host", "places.example.com".parse().unwrap());
        let out = forwarded_headers(&headers);
        assert_eq!(out.len(), 4);
        assert!(out.contains_key("x-identity-auth-chain-0"));
        assert!(out.contains_key("x-identity-timestamp"));
        assert!(out.contains_key("authorization"));
        assert!(out.contains_key("x-original-path"));
        assert!(!out.contains_key("cookie"));
        assert!(!out.contains_key("host"));
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::json;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    async fn events_service(reply: Value) -> Events {
        let app = Router::new().route(
            "/api/events/search",
            post(move || {
                let reply = reply.clone();
                async move { Json(reply) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Events::new(base)
    }

    fn world_party() -> Value {
        json!({
            "id": "ev-1",
            "place_id": "upstream-uuid",
            "server": "my-world.dcl.eth",
            "world": true,
            "name": "World Party",
            "next_start_at": "2030-01-01T10:00:00Z"
        })
    }

    #[tokio::test]
    async fn a_completed_live_search_names_a_world_by_its_server_and_caches_every_answer() {
        let movie_night = json!({
            "id": "ev-2",
            "place_id": "place-1",
            "server": "peer.decentraland.org",
            "world": false,
            "name": "Movie Night"
        });
        let events = events_service(
            json!({ "ok": true, "data": { "events": [world_party(), movie_night] } }),
        )
        .await;
        let asked = ids(&["place-1", "My-World.dcl.eth", "place-2"]);
        let live = events.check_live_events(&asked).await;
        assert_eq!(live["My-World.dcl.eth"].as_deref(), Some("World Party"));
        assert_eq!(live["place-1"].as_deref(), Some("Movie Night"));
        assert_eq!(live["place-2"], None);
        assert_eq!(
            events.cached_live_event("My-World.dcl.eth"),
            Some(Some("World Party".to_string()))
        );
        assert_eq!(
            events.cached_live_event("place-2"),
            Some(None),
            "a completed search caches the absence of a live event"
        );
    }

    #[tokio::test]
    async fn a_failed_live_search_caches_no_absence() {
        let events = Events::new("http://127.0.0.1:9".into());
        let live = events.check_live_events(&ids(&["place-1"])).await;
        assert_eq!(live["place-1"], None);
        assert_eq!(
            events.cached_live_event("place-1"),
            None,
            "an unreachable events service must not suppress the next retry"
        );
    }

    #[tokio::test]
    async fn a_next_event_search_names_a_world_by_its_server() {
        let events =
            events_service(json!({ "ok": true, "data": { "events": [world_party()] } })).await;
        let next = events
            .next_events(&ids(&["My-World.dcl.eth", "place-1"]))
            .await;
        assert_eq!(next["My-World.dcl.eth"].id, "ev-1");
        assert!(!next.contains_key("place-1"));
        assert_eq!(events.cached_next_event("place-1"), Some(None));
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn a_bare_root_and_a_legacy_api_base_spell_the_same_routes() {
        let bare = Events::new("http://127.0.0.1:5135".into());
        let legacy = Events::new("https://events.decentraland.zone/api".into());
        assert_eq!(
            bare.url("/api/events/search?list=live"),
            "http://127.0.0.1:5135/api/events/search?list=live"
        );
        assert_eq!(
            legacy.url("/api/events/search?list=live"),
            "https://events.decentraland.zone/api/events/search?list=live"
        );
        assert_eq!(
            destination_events_url(&bare.base_url, "x"),
            "http://127.0.0.1:5135/v1/destinations/x/events"
        );
        assert_eq!(
            destination_events_url(&legacy.base_url, "x"),
            "https://events.decentraland.zone/v1/destinations/x/events"
        );
        assert_eq!(
            service_root("http://127.0.0.1:5135/api/"),
            "http://127.0.0.1:5135"
        );
        assert_eq!(
            service_root("http://127.0.0.1:5135/"),
            "http://127.0.0.1:5135"
        );
    }

    #[test]
    fn a_destination_id_is_always_one_path_segment() {
        for (id, encoded) in [
            ("a/b", "a%2Fb"),
            ("x?y=1", "x%3Fy%3D1"),
            ("../../api/ping", "%2E%2E%2F%2E%2E%2Fapi%2Fping"),
            ("x#y", "x%23y"),
            ("my-world.dcl.eth", "my%2Dworld%2Edcl%2Eeth"),
        ] {
            assert_eq!(
                destination_events_url("http://events", id),
                format!("http://events/v1/destinations/{encoded}/events"),
                "{id}"
            );
        }
    }
}
