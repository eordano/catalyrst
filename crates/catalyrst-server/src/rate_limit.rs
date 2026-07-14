//! Fixed-window rate limit on POST /entities, keyed per client.
//!
//! Env: `POST_ENTITIES_RATE_LIMIT_MAX` (default 200), `POST_ENTITIES_RATE_LIMIT_WINDOW_SECONDS`
//! (default 60), `TRUSTED_CLIENT_IP_HEADER` (unset by default, matching upstream). Unset keys on
//! the socket peer, which is correct only for a directly exposed process -- and this process never
//! is: POST /entities arrives through the nginx front, so the peer is the front for every request
//! and the per-client limit collapses into one global budget shared by every deployer. Deployment
//! note: the catalyrst-live unit environment must set `TRUSTED_CLIENT_IP_HEADER=x-real-ip`; the
//! front already writes `X-Real-IP $remote_addr` via its reverse-proxy config.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use dashmap::DashMap;
use parking_lot::RwLock;

const DEFAULT_MAX: u64 = 200;
const DEFAULT_WINDOW_SECONDS: u64 = 60;
const MAX_WINDOW_SECONDS: u64 = 86_400;
const MAX_TRACKED_CLIENTS: usize = 50_000;
const FALLBACK_MAX_DIVISOR: u64 = 10;
const FALLBACK_IDENTITY: &str = "unidentified-client";

pub struct PostEntitiesRateLimiter {
    max: u64,
    fallback_max: u64,
    window_ms: u64,
    trusted_client_ip_header: Option<HeaderName>,
    generations: RwLock<Generations>,
}

#[derive(Default)]
struct Generations {
    active: DashMap<String, WindowCounter>,
    previous: DashMap<String, WindowCounter>,
}

struct WindowCounter {
    window_id: u64,
    count: u64,
}

impl WindowCounter {
    fn tick(&mut self, window_id: u64) -> u64 {
        if self.window_id != window_id {
            self.window_id = window_id;
            self.count = 0;
        }
        self.count += 1;
        self.count
    }
}

struct Decision {
    allowed: bool,
    retry_after_seconds: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum KeySource {
    TrustedHeader,
    Socket,
    Fallback,
}

impl KeySource {
    fn label(self) -> &'static str {
        match self {
            KeySource::TrustedHeader => "trusted-header",
            KeySource::Socket => "socket",
            KeySource::Fallback => "fallback",
        }
    }
}

fn parse_positive(name: &str, value: Option<&str>, default: u64) -> u64 {
    let Some(raw) = value else { return default };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default;
    }
    match trimmed.parse::<u64>() {
        Ok(n) if n > 0 => n,
        // Never floor a 0 to 1: that is a one-request-per-window outage that looks configured.
        _ => panic!("invalid {name}: expected a positive integer but got {raw:?}"),
    }
}

fn parse_header_name(name: &str, value: Option<&str>) -> Option<HeaderName> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match HeaderName::from_bytes(trimmed.as_bytes()) {
        Ok(header) => Some(header),
        Err(_) => panic!("invalid {name}: expected an HTTP header name but got {raw:?}"),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Phases each identity's fixed window by a stable per-identity offset, so one Retry-After
// discloses only that caller's own boundary and counters don't all reset at the same instant.
fn phase_offset(identity: &str, window_ms: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hasher);
    hasher.finish() % window_ms
}

fn canonical_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V6(v6) => v6.to_canonical().to_string(),
        v4 => v4.to_string(),
    }
}

fn parse_client_ip(value: &str) -> Option<IpAddr> {
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Ok(sock) = value.parse::<SocketAddr>() {
        return Some(sock.ip());
    }
    value.strip_prefix('[')?.strip_suffix(']')?.parse().ok()
}

// Rightmost non-empty entry: every proxy appends, so the rightmost hop was written by our own
// infrastructure while the leftmost is whatever the client chose to send.
fn client_ip_from_forwarded(value: &str) -> Option<String> {
    let hop = value
        .rsplit(',')
        .map(str::trim)
        .find(|hop| !hop.is_empty())?;
    parse_client_ip(hop).map(canonical_ip)
}

impl PostEntitiesRateLimiter {
    pub fn from_env() -> Self {
        let max = parse_positive(
            "POST_ENTITIES_RATE_LIMIT_MAX",
            std::env::var("POST_ENTITIES_RATE_LIMIT_MAX")
                .ok()
                .as_deref(),
            DEFAULT_MAX,
        );
        let window_seconds = parse_positive(
            "POST_ENTITIES_RATE_LIMIT_WINDOW_SECONDS",
            std::env::var("POST_ENTITIES_RATE_LIMIT_WINDOW_SECONDS")
                .ok()
                .as_deref(),
            DEFAULT_WINDOW_SECONDS,
        );
        if window_seconds > MAX_WINDOW_SECONDS {
            panic!("invalid POST_ENTITIES_RATE_LIMIT_WINDOW_SECONDS: {window_seconds} exceeds a day and is almost certainly milliseconds by mistake");
        }
        let trusted_client_ip_header = parse_header_name(
            "TRUSTED_CLIENT_IP_HEADER",
            std::env::var("TRUSTED_CLIENT_IP_HEADER").ok().as_deref(),
        );
        // Config-time only: a request-driven warning would fire on a header any client can forge.
        if trusted_client_ip_header.is_none() {
            tracing::warn!(
                "TRUSTED_CLIENT_IP_HEADER is unset, so POST /entities is rate limited by socket address. \
                 That is correct only when this process is reached directly; behind a proxy every client \
                 shares one budget. Behind the nginx front set TRUSTED_CLIENT_IP_HEADER=x-real-ip (the \
                 front writes X-Real-IP). Watch the key_source label on rate_limiter_requests_total to \
                 tell which is happening."
            );
        }
        Self::new(max, window_seconds, trusted_client_ip_header)
    }

    fn new(max: u64, window_seconds: u64, trusted_client_ip_header: Option<HeaderName>) -> Self {
        Self {
            max,
            fallback_max: (max / FALLBACK_MAX_DIVISOR).max(1),
            window_ms: window_seconds * 1000,
            trusted_client_ip_header,
            generations: RwLock::new(Generations::default()),
        }
    }

    fn identity(&self, request: &Request) -> (String, KeySource) {
        if let Some(name) = &self.trusted_client_ip_header {
            // Joined across repeated lines before the rightmost-hop walk, matching upstream's
            // Headers.get: a front that appends its own line after a client-supplied one must win.
            let mut present = false;
            let mut joined = String::new();
            for value in request.headers().get_all(name) {
                present = true;
                if let Ok(text) = value.to_str() {
                    if !joined.is_empty() {
                        joined.push(',');
                    }
                    joined.push_str(text);
                }
            }
            if let Some(ip) = client_ip_from_forwarded(&joined) {
                return (ip, KeySource::TrustedHeader);
            }
            metrics::counter!(
                "rate_limiter_client_address_issues_total",
                "issue" => if present { "trusted-header-unusable" } else { "trusted-header-missing" }
            )
            .increment(1);
        }
        if let Some(info) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
            return (canonical_ip(info.0.ip()), KeySource::Socket);
        }
        metrics::counter!(
            "rate_limiter_client_address_issues_total",
            "issue" => "no-client-address"
        )
        .increment(1);
        (FALLBACK_IDENTITY.to_owned(), KeySource::Fallback)
    }

    fn decide(&self, identity: &str, max: u64) -> Decision {
        self.decide_at(identity, max, now_ms())
    }

    fn decide_at(&self, identity: &str, max: u64, now_ms: u64) -> Decision {
        let offset = phase_offset(identity, self.window_ms);
        let window_id = (now_ms + offset) / self.window_ms;
        let reset_at_ms = (window_id + 1) * self.window_ms - offset;
        // Never 0: some clients read Retry-After: 0 as "retry immediately".
        let retry_after_seconds = ((reset_at_ms - now_ms).div_ceil(1000)).max(1);
        Decision {
            allowed: self.count_in_window(identity, window_id) <= max,
            retry_after_seconds,
        }
    }

    // A full table rotates generations instead of sweeping or failing open: the active map becomes
    // the previous one -- still consulted, so a tracked client keeps its spent budget -- and new
    // identities land in a fresh map. Limiting never switches off, per-request work stays O(1),
    // and memory is bounded by two generations.
    fn count_in_window(&self, identity: &str, window_id: u64) -> u64 {
        loop {
            let generations = self.generations.read();
            if let Some(mut counter) = generations.active.get_mut(identity) {
                return counter.tick(window_id);
            }
            if generations.active.len() >= MAX_TRACKED_CLIENTS {
                drop(generations);
                let mut generations = self.generations.write();
                if generations.active.len() >= MAX_TRACKED_CLIENTS {
                    generations.previous = std::mem::take(&mut generations.active);
                }
                continue;
            }
            let carried = match generations.previous.remove(identity) {
                Some((_, counter)) => counter,
                None => WindowCounter {
                    window_id,
                    count: 0,
                },
            };
            return generations
                .active
                .entry(identity.to_owned())
                .or_insert(carried)
                .tick(window_id);
        }
    }
}

pub async fn post_entities_rate_limit(
    State(limiter): State<Arc<PostEntitiesRateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    let (identity, source) = limiter.identity(&request);
    let max = match source {
        KeySource::Fallback => limiter.fallback_max,
        _ => limiter.max,
    };
    let decision = limiter.decide(&identity, max);
    // Metrics, never a log line: a throttled client retries, so a line per rejection is write
    // amplification driven by the abuse being blocked.
    metrics::counter!(
        "rate_limiter_requests_total",
        "outcome" => if decision.allowed { "allowed" } else { "limited" },
        "key_source" => source.label()
    )
    .increment(1);

    if decision.allowed {
        return next.run(request).await;
    }

    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ApiErrorBody::new("Too many requests")),
    )
        .into_response();
    if let Ok(value) = decision.retry_after_seconds.to_string().parse() {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn parse_positive_defaults_and_accepts() {
        assert_eq!(parse_positive("X", None, 200), 200);
        assert_eq!(parse_positive("X", Some(""), 200), 200);
        assert_eq!(parse_positive("X", Some(" 7 "), 200), 7);
    }

    #[test]
    #[should_panic(expected = "invalid X")]
    fn parse_positive_rejects_zero() {
        parse_positive("X", Some("0"), 200);
    }

    #[test]
    #[should_panic(expected = "invalid X")]
    fn parse_positive_rejects_garbage() {
        parse_positive("X", Some("256MB"), 200);
    }

    #[test]
    #[should_panic(expected = "invalid H")]
    fn parse_header_name_rejects_invalid() {
        parse_header_name("H", Some("not a header"));
    }

    #[test]
    fn parse_header_name_trims_and_accepts() {
        assert_eq!(
            parse_header_name("H", Some(" X-Real-IP ")),
            Some(HeaderName::from_static("x-real-ip"))
        );
        assert_eq!(parse_header_name("H", Some("  ")), None);
        assert_eq!(parse_header_name("H", None), None);
    }

    #[test]
    fn forwarded_header_takes_rightmost_entry() {
        assert_eq!(
            client_ip_from_forwarded("203.0.113.7"),
            Some("203.0.113.7".to_owned())
        );
        assert_eq!(
            client_ip_from_forwarded("198.51.100.1, 203.0.113.7"),
            Some("203.0.113.7".to_owned())
        );
        assert_eq!(
            client_ip_from_forwarded("198.51.100.1, 203.0.113.7, "),
            Some("203.0.113.7".to_owned())
        );
        assert_eq!(client_ip_from_forwarded("not-an-ip"), None);
        assert_eq!(client_ip_from_forwarded(""), None);
    }

    #[test]
    fn addresses_are_canonicalized_to_one_bucket_per_client() {
        assert_eq!(
            client_ip_from_forwarded("203.0.113.7:4321"),
            Some("203.0.113.7".to_owned())
        );
        assert_eq!(
            client_ip_from_forwarded("::ffff:203.0.113.7"),
            Some("203.0.113.7".to_owned())
        );
        assert_eq!(
            client_ip_from_forwarded("[2001:DB8:0:0:0:0:0:1]:443"),
            Some("2001:db8::1".to_owned())
        );
        assert_eq!(
            client_ip_from_forwarded("2001:db8:0:0:0:0:0:1"),
            Some("2001:db8::1".to_owned())
        );
    }

    #[test]
    fn counts_reject_over_budget_and_reset_next_window() {
        let limiter = PostEntitiesRateLimiter::new(2, 60, None);
        let t0 = 1_000_000_000_000;
        assert!(limiter.decide_at("a", 2, t0).allowed);
        assert!(limiter.decide_at("a", 2, t0).allowed);
        let rejected = limiter.decide_at("a", 2, t0);
        assert!(!rejected.allowed);
        assert!(rejected.retry_after_seconds >= 1 && rejected.retry_after_seconds <= 60);
        assert!(limiter.decide_at("b", 2, t0).allowed);
        assert!(limiter.decide_at("a", 2, t0 + 2 * 60_000).allowed);
    }

    #[test]
    fn saturation_rotates_generations_and_keeps_limiting() {
        let limiter = PostEntitiesRateLimiter::new(2, 60, None);
        let t0 = 1_000_000_000_000;
        assert!(limiter.decide_at("carried", 2, t0).allowed);
        assert!(limiter.decide_at("carried", 2, t0).allowed);
        for i in 0..MAX_TRACKED_CLIENTS {
            limiter.decide_at(&format!("flood-{i}"), 2, t0);
        }
        assert!(limiter.decide_at("newcomer", 2, t0).allowed);
        assert!(limiter.decide_at("newcomer", 2, t0).allowed);
        assert!(!limiter.decide_at("newcomer", 2, t0).allowed);
        assert!(!limiter.decide_at("carried", 2, t0).allowed);
    }

    #[test]
    fn trusted_header_later_line_wins() {
        let limiter =
            PostEntitiesRateLimiter::new(2, 60, Some(HeaderName::from_static("x-real-ip")));
        let request = Request::builder()
            .method("POST")
            .uri("/entities")
            .header("X-Real-IP", "6.6.6.6")
            .header("X-Real-IP", "198.51.100.1, 203.0.113.7")
            .body(Body::empty())
            .unwrap();
        let (identity, source) = limiter.identity(&request);
        assert_eq!(identity, "203.0.113.7");
        assert!(source == KeySource::TrustedHeader);
    }

    #[test]
    fn trusted_header_unusable_last_line_never_falls_back_to_client_line() {
        let limiter =
            PostEntitiesRateLimiter::new(2, 60, Some(HeaderName::from_static("x-real-ip")));
        let request = Request::builder()
            .method("POST")
            .uri("/entities")
            .header("X-Real-IP", "6.6.6.6")
            .header("X-Real-IP", "not-an-ip")
            .body(Body::empty())
            .unwrap();
        let (identity, source) = limiter.identity(&request);
        assert_eq!(identity, FALLBACK_IDENTITY);
        assert!(source == KeySource::Fallback);
    }

    fn limited_app(limiter: Arc<PostEntitiesRateLimiter>) -> Router {
        Router::new()
            .route("/entities", post(|| async { "deployed" }))
            .route_layer(axum::middleware::from_fn_with_state(
                limiter,
                post_entities_rate_limit,
            ))
    }

    fn deploy_req(ip: &str) -> Request {
        Request::builder()
            .method("POST")
            .uri("/entities")
            .header("X-Real-IP", ip)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn over_budget_gets_429_with_retry_after_and_error_envelope() {
        let limiter = Arc::new(PostEntitiesRateLimiter::new(
            1,
            60,
            Some(HeaderName::from_static("x-real-ip")),
        ));
        let app = limited_app(limiter);

        let ok = app
            .clone()
            .oneshot(deploy_req("203.0.113.7"))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        assert!(ok.headers().get(header::RETRY_AFTER).is_none());

        let throttled = app
            .clone()
            .oneshot(deploy_req("203.0.113.7"))
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after: u64 = throttled
            .headers()
            .get(header::RETRY_AFTER)
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(retry_after >= 1);
        let bytes = axum::body::to_bytes(throttled.into_body(), 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "Too many requests");

        let other_client = app.oneshot(deploy_req("198.51.100.9")).await.unwrap();
        assert_eq!(other_client.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn unidentified_clients_share_one_tightened_bucket() {
        let limiter = Arc::new(PostEntitiesRateLimiter::new(30, 60, None));
        let app = limited_app(limiter);
        let mut last = StatusCode::OK;
        for _ in 0..4 {
            let req = Request::builder()
                .method("POST")
                .uri("/entities")
                .body(Body::empty())
                .unwrap();
            last = app.clone().oneshot(req).await.unwrap().status();
        }
        assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    }
}
