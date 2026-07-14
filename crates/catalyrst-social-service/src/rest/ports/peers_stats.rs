use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// How long one `/peers` snapshot is served before re-probing. Upstream keeps a
/// synchronizer-refreshed redis set; this local TTL plays the same role of
/// decoupling request rate from probe rate.
const PEERS_CACHE_TTL: Duration = Duration::from_secs(10);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Wire shape of the archipelago stats `/peers` endpoint (`{ok, peers: [{id,
/// address, ...}]}`). Upstream social-service-ea maps `peer.id`; `address` is
/// the fallback because catalyrst-archipelago serves both with equal value.
#[derive(Deserialize)]
struct PeersResponse {
    #[serde(default)]
    peers: Vec<PeerEntry>,
}

#[derive(Deserialize)]
struct PeerEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

/// Reads "who is connected to comms right now" from the archipelago stats
/// `/peers` endpoint -- the same source upstream's archipelagoStats adapter
/// polls. Presence is advisory: every failure degrades to the empty set, so an
/// unreachable presence source renders members as offline rather than failing
/// the request.
pub struct PeersStatsClient {
    client: reqwest::Client,
    base_url: String,
    cache: Mutex<Option<(Instant, Vec<String>)>>,
}

impl PeersStatsClient {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            cache: Mutex::new(None),
        }
    }

    /// Lowercased addresses currently connected to comms. Never errors: a
    /// failed probe yields the empty set. Both outcomes are cached for
    /// `PEERS_CACHE_TTL`, so a dead presence source costs at most one bounded
    /// probe per TTL window instead of one per request.
    pub async fn connected_peers(&self) -> Vec<String> {
        if let Some(cached) = self.cache_get() {
            return cached;
        }
        let peers = match self.fetch().await {
            Ok(peers) => peers,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    url = %self.base_url,
                    "archipelago peers unavailable; reporting all members offline"
                );
                Vec::new()
            }
        };
        self.cache_put(peers.clone());
        peers
    }

    fn cache_get(&self) -> Option<Vec<String>> {
        let cache = self.cache.lock().expect("peers cache poisoned");
        cache
            .as_ref()
            .filter(|(at, _)| at.elapsed() < PEERS_CACHE_TTL)
            .map(|(_, peers)| peers.clone())
    }

    fn cache_put(&self, peers: Vec<String>) {
        let mut cache = self.cache.lock().expect("peers cache poisoned");
        *cache = Some((Instant::now(), peers));
    }

    async fn fetch(&self) -> Result<Vec<String>, anyhow::Error> {
        let url = format!("{}/peers", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("archipelago stats returned status {}", status.as_u16());
        }
        let body: PeersResponse = resp.json().await?;
        Ok(peer_addresses(body))
    }
}

/// `id` first (the field upstream consumes), `address` as fallback; lowercased
/// and deduplicated so the SQL `ANY($2)` filter sees one canonical form.
fn peer_addresses(resp: PeersResponse) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for peer in resp.peers {
        let Some(addr) = peer.id.or(peer.address) else {
            continue;
        };
        let addr = addr.to_lowercase();
        if addr.is_empty() || !seen.insert(addr.clone()) {
            continue;
        }
        out.push(addr);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn parse(json: &str) -> Vec<String> {
        peer_addresses(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn parses_ids_lowercased_and_deduplicated() {
        let json = r#"{"ok":true,"peers":[
            {"id":"0xAB","address":"0xAB"},
            {"id":"0xab"},
            {"address":"0xCD"},
            {"lastPing":1},
            {"id":""}
        ]}"#;
        assert_eq!(parse(json), vec!["0xab".to_string(), "0xcd".to_string()]);
    }

    #[test]
    fn missing_peers_key_parses_as_empty() {
        assert!(parse(r#"{"ok":true}"#).is_empty());
    }

    async fn serve(status: u16, body: &'static str, hits: Arc<AtomicUsize>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route(
            "/peers",
            axum::routing::get(move || {
                hits.fetch_add(1, Ordering::SeqCst);
                async move {
                    axum::response::Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap()
                }
            }),
        );
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn returns_connected_addresses_and_serves_repeats_from_cache() {
        let hits = Arc::new(AtomicUsize::new(0));
        let base = serve(
            200,
            r#"{"ok":true,"peers":[{"id":"0xAA"},{"id":"0xbb"}]}"#,
            hits.clone(),
        )
        .await;
        let client = PeersStatsClient::new(base);

        let peers = client.connected_peers().await;
        assert_eq!(peers, vec!["0xaa".to_string(), "0xbb".to_string()]);

        let again = client.connected_peers().await;
        assert_eq!(again, peers);
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "second read is cache-served"
        );
    }

    #[tokio::test]
    async fn upstream_error_yields_empty_not_error() {
        let hits = Arc::new(AtomicUsize::new(0));
        let base = serve(500, "boom", hits.clone()).await;
        let client = PeersStatsClient::new(base);
        assert!(client.connected_peers().await.is_empty());
        client.connected_peers().await;
        assert_eq!(hits.load(Ordering::SeqCst), 1, "failure is cached too");
    }

    #[tokio::test]
    async fn unreachable_source_yields_empty() {
        let client = PeersStatsClient::new("http://127.0.0.1:1".to_string());
        assert!(client.connected_peers().await.is_empty());
    }
}
