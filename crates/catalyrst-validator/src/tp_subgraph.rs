use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::debug;

pub fn ensure_tls_or_loopback(url: &str, env_name: &str) {
    if !url.starts_with("http://") {
        return;
    }
    let host = url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]");
    if !is_loopback {
        panic!(
            "{env_name} is plaintext http:// ({url}); subgraph responses gate \
             marketplace access checks and require TLS (https://) or a \
             loopback host. Refusing to start."
        );
    }
}

/// Bounds the third-party-root memo. `(tp_id, block)` pairs are few in practice
/// (one collection's batch resolves to one or a handful of finalized L2 blocks),
/// so this cap is only a safety valve; on overflow the whole map is cleared and
/// entries are simply re-fetched.
const TP_ROOT_CACHE_CAP: usize = 1024;

type RootCache = Mutex<HashMap<(String, u64), Option<[u8; 32]>>>;

pub struct TpSubgraph {
    client: reqwest::Client,
    blocks_l2_url: String,
    tpr_url: String,
    /// Memo of `(tp_id, block) -> decoded root option`. A finalized L2 block is
    /// immutable, so a successfully-fetched root (or confirmed-absent root) for
    /// a given pair is a byte-identical fact on every replay -- safe to cache.
    /// Transport failures are NOT inserted (see `third_party_root_cached`), so a
    /// transient subgraph outage can never pin a false rejection.
    root_cache: RootCache,
}

impl TpSubgraph {
    pub fn new(blocks_l2_url: String, tpr_url: String) -> Self {
        ensure_tls_or_loopback(&blocks_l2_url, "BLOCKS_L2_SUBGRAPH_URL");
        ensure_tls_or_loopback(&tpr_url, "THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(2))
            .build()
            .expect("reqwest client with timeout should build");

        Self {
            client,
            blocks_l2_url,
            tpr_url,
            root_cache: Mutex::new(HashMap::new()),
        }
    }

    async fn graphql(&self, url: &str, query: &str, variables: Value) -> Option<Value> {
        let resp = self
            .client
            .post(url)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            debug!(url, status = %resp.status(), "subgraph query non-success");
            return None;
        }
        let body: Value = resp.json().await.ok()?;
        body.get("data").cloned()
    }

    pub async fn block_for_timestamp(&self, timestamp_ms: i64) -> Option<u64> {
        let timestamp_sec = (timestamp_ms as f64 / 1000.0).ceil() as i64 + 8;
        let timestamp_5min = (timestamp_sec - 60 * 5 - 7).max(0);
        let query = r#"query getBlockForTimestampRange($timestamp: Int!, $timestamp5Min: Int!) {
            max: blocks(where: {timestamp_gte: $timestamp5Min, timestamp_lte: $timestamp}, first: 1, orderBy: timestamp, orderDirection: desc) { number }
            min: blocks(where: {timestamp_gte: $timestamp5Min, timestamp_lte: $timestamp}, first: 1, orderBy: timestamp, orderDirection: asc) { number }
        }"#;
        let variables = json!({ "timestamp": timestamp_sec, "timestamp5Min": timestamp_5min });

        for attempt in 1..=BLOCK_FETCH_MAX_RETRIES {
            if let Some(block) = self.block_for_timestamp_once(query, &variables).await {
                return Some(block);
            }
            if attempt < BLOCK_FETCH_MAX_RETRIES {
                let base = block_fetch_backoff_base(attempt);
                let backoff = base + block_fetch_jitter(base);
                debug!(
                    attempt,
                    backoff_ms = backoff,
                    "retrying L2 block-for-timestamp lookup"
                );
                tokio::time::sleep(Duration::from_millis(backoff)).await;
            }
        }
        debug!("L2 block-for-timestamp lookup exhausted retries");
        None
    }

    async fn block_for_timestamp_once(&self, query: &str, variables: &Value) -> Option<u64> {
        let data = self
            .graphql(&self.blocks_l2_url, query, variables.clone())
            .await?;
        let num = data
            .get("max")
            .and_then(|m| m.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("number"))?;
        parse_u64(num)
    }

    pub async fn fetch_all_third_parties(&self) -> Option<Vec<(String, Option<String>, bool)>> {
        const ZERO_ROOT: &str =
            "0x0000000000000000000000000000000000000000000000000000000000000000";
        let query = "{ thirdParties(first: 1000) { id root isApproved } }";
        let data = self.graphql(&self.tpr_url, query, json!({})).await?;
        let arr = data.get("thirdParties")?.as_array()?;
        let mut out = Vec::with_capacity(arr.len());
        for tp in arr {
            let Some(id) = tp.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let root = tp
                .get("root")
                .and_then(|r| r.as_str())
                .filter(|s| !s.is_empty() && *s != ZERO_ROOT)
                .map(|s| s.to_string());
            let is_approved = tp
                .get("isApproved")
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            out.push((id.to_string(), root, is_approved));
        }
        Some(out)
    }

    pub async fn third_party_root(&self, third_party_id: &str, block: u64) -> Option<[u8; 32]> {
        third_party_root_cached(&self.root_cache, third_party_id, block, || async {
            let query = r#"query MerkleRoot($id: ID!, $block: Int!) {
                thirdParties(where: { id: $id, isApproved: true }, block: { number: $block }, first: 1) { root }
            }"#;
            // `graphql` returns None only on a transport/non-success error -- the
            // ONE case we must not memoize. `Some(data)` is a confirmed answer
            // (a root, or a confirmed-absent root), which is cacheable.
            let data = self
                .graphql(
                    &self.tpr_url,
                    query,
                    json!({ "id": third_party_id, "block": block }),
                )
                .await?;
            let root = data
                .get("thirdParties")
                .and_then(|t| t.as_array())
                .and_then(|a| a.first())
                .and_then(|tp| tp.get("root"))
                .and_then(|r| r.as_str())
                .and_then(crate::merkle::decode_hash32);
            Some(root)
        })
        .await
    }
}

/// Memoize third-party root lookups on `(tp_id, block)`. The `fetch` closure
/// returns `None` for a transport failure (NOT cached, so a transient outage
/// never pins a false rejection) and `Some(root_opt)` for a confirmed answer
/// (cached -- the block is finalized, so the answer is immutable). Cache hits
/// return the stored `Option<[u8; 32]>` without any round trip.
async fn third_party_root_cached<F, Fut>(
    cache: &RootCache,
    third_party_id: &str,
    block: u64,
    fetch: F,
) -> Option<[u8; 32]>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Option<Option<[u8; 32]>>>,
{
    let key = (third_party_id.to_string(), block);
    if let Some(cached) = cache.lock().await.get(&key) {
        return *cached;
    }
    match fetch().await {
        Some(result) => {
            let mut guard = cache.lock().await;
            if guard.len() >= TP_ROOT_CACHE_CAP {
                guard.clear();
            }
            guard.insert(key, result);
            result
        }
        None => None,
    }
}

const BLOCK_FETCH_MAX_RETRIES: u32 = 3;
const BLOCK_FETCH_BASE_DELAY_MS: u64 = 100;

fn block_fetch_backoff_base(attempt: u32) -> u64 {
    BLOCK_FETCH_BASE_DELAY_MS * (1u64 << (attempt - 1))
}

fn block_fetch_jitter(base: u64) -> u64 {
    if base == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % base
}

fn parse_u64(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // [Performance] third_party_root memoizes on (tp_id, block): N back-to-back
    // lookups for the same finalized block issue exactly ONE subgraph round trip.
    // Red-check: reverting the cache (fetch on every call) makes the count == N.
    #[tokio::test]
    async fn tp_root_fetched_once_per_block() {
        let cache: RootCache = Mutex::new(HashMap::new());
        let fetches = AtomicUsize::new(0);
        let root = [7u8; 32];

        for _ in 0..10 {
            let got = third_party_root_cached(&cache, "urn:tp:foo", 42, || async {
                fetches.fetch_add(1, Ordering::SeqCst);
                Some(Some(root)) // confirmed answer
            })
            .await;
            assert_eq!(got, Some(root));
        }
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            1,
            "the finalized-block root must be fetched at most once"
        );
    }

    // [Performance] + correctness: a transport failure (fetch -> None) is NOT
    // cached, so a later call re-fetches and can succeed; and once a confirmed
    // answer is cached, further calls issue no round trip. Guards against a
    // transient subgraph outage pinning a false rejection.
    #[tokio::test]
    async fn tp_root_transport_failure_not_cached() {
        let cache: RootCache = Mutex::new(HashMap::new());
        let fetches = AtomicUsize::new(0);
        let root = [9u8; 32];

        // 1st: transport failure -> None, must not cache.
        let a = third_party_root_cached(&cache, "urn:tp:bar", 7, || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            None
        })
        .await;
        assert_eq!(a, None);

        // 2nd: succeeds -> caches the confirmed root.
        let b = third_party_root_cached(&cache, "urn:tp:bar", 7, || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Some(Some(root))
        })
        .await;
        assert_eq!(b, Some(root));

        // 3rd: served from cache, no fetch.
        let c = third_party_root_cached(&cache, "urn:tp:bar", 7, || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Some(Some([0u8; 32]))
        })
        .await;
        assert_eq!(c, Some(root));

        assert_eq!(
            fetches.load(Ordering::SeqCst),
            2,
            "only the failed attempt and the first success fetch; the cached hit must not"
        );
    }

    #[test]
    fn retry_budget_is_bounded() {
        assert_eq!(BLOCK_FETCH_MAX_RETRIES, 3);
        const {
            assert!(
                BLOCK_FETCH_MAX_RETRIES > 1,
                "there must be at least one retry after the initial attempt"
            )
        };
        assert_eq!(BLOCK_FETCH_MAX_RETRIES - 1, 2);
    }

    #[test]
    fn backoff_base_doubles_each_attempt() {
        assert_eq!(block_fetch_backoff_base(1), 100);
        assert_eq!(block_fetch_backoff_base(2), 200);
        assert_eq!(block_fetch_backoff_base(3), 400);
        for attempt in 1..BLOCK_FETCH_MAX_RETRIES {
            assert!(block_fetch_backoff_base(attempt) < block_fetch_backoff_base(attempt + 1));
        }
    }

    #[test]
    fn jitter_stays_within_full_jitter_bounds() {
        assert_eq!(block_fetch_jitter(0), 0);

        for attempt in 1..BLOCK_FETCH_MAX_RETRIES {
            let base = block_fetch_backoff_base(attempt);
            for _ in 0..1000 {
                let jitter = block_fetch_jitter(base);
                assert!(jitter < base, "jitter {jitter} must be < base {base}");
                let total = base + jitter;
                assert!(total >= base && total < 2 * base);
            }
        }
    }
}
