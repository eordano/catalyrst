#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};

use catalyrst_credits::auth_chain::build_payload;
use catalyrst_credits::ports::credits::CreditsComponent;
use catalyrst_credits::ports::pricing::PricingClient;
use catalyrst_credits::{AppState, AppStateInner};

static WALLET_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn scratch_wallet() -> PrivateKeySigner {
    let mut h = Sha256::new();
    h.update(std::process::id().to_le_bytes());
    h.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes(),
    );
    h.update(WALLET_COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let key: [u8; 32] = h.finalize().into();
    PrivateKeySigner::from_slice(&key).expect("wallet from bytes")
}

pub fn wallet_addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

fn link_json(kind: &str, payload: &str, signature: &str) -> String {
    serde_json::json!({ "type": kind, "payload": payload, "signature": signature }).to_string()
}

pub async fn signed_headers(wallet: &PrivateKeySigner, method: &str, path: &str) -> HeaderMap {
    let root_addr = wallet_addr(wallet);
    let ephemeral = scratch_wallet();
    let ephemeral_addr = wallet_addr(&ephemeral);

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = wallet
        .sign_message(ephemeral_payload.as_bytes())
        .await
        .unwrap()
        .to_string();

    let ts_ms = chrono_now_ms();
    let canonical = build_payload(method, path, &ts_ms.to_string(), "{}");
    let entity_sig = ephemeral
        .sign_message(canonical.as_bytes())
        .await
        .unwrap()
        .to_string();

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-0"),
        HeaderValue::from_str(&link_json("SIGNER", &root_addr, "")).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-1"),
        HeaderValue::from_str(&link_json(
            "ECDSA_EPHEMERAL",
            &ephemeral_payload,
            &ephemeral_sig,
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&link_json("ECDSA_SIGNED_ENTITY", &canonical, &entity_sig)).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-timestamp"),
        HeaderValue::from_str(&ts_ms.to_string()).unwrap(),
    );
    headers
}

fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub async fn pool() -> Option<sqlx::PgPool> {
    let url = catalyrst_testgate::require_pg("CREDITS_TEST_PG_CONNECTION_STRING")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("test PG unreachable");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("test PG migrations failed");
    Some(pool)
}

pub fn lazy_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/never")
        .expect("lazy pool")
}

pub fn test_state(pool: sqlx::PgPool, mock_card: bool) -> AppState {
    test_state_with_market(pool, mock_card, "http://127.0.0.1:1", "secondary")
}

pub fn test_state_with_market(
    pool: sqlx::PgPool,
    mock_card: bool,
    market_base_url: &str,
    fulfillment_mode: &str,
) -> AppState {
    let http = reqwest::Client::new();
    Arc::new(AppStateInner {
        credits: CreditsComponent::new(pool.clone()),
        admin_token: None,
        captcha_provider: None,
        stripe: None,
        stripe_webhook_secret: None,
        mock_card,
        credits_currency: "usd".into(),
        pricing: PricingClient::new(
            http.clone(),
            market_base_url.into(),
            market_base_url.into(),
            0,
            3600,
        ),
        checkout_fulfillment_mode: fulfillment_mode.into(),
        require_purchase_intent: false,
        economy_base_url: "http://127.0.0.1:1".into(),
        economy_admin_token: Some("test-token".into()),
        escrow_address: Some("0x0000000000000000000000000000000000000001".into()),
        usage_grants_pool: Some(pool),
        economy_http: http,
        quote_cache: Default::default(),
        credits_signer_key: None,
        credits_manager_contract: None,
        checkout_success_url: String::new(),
        checkout_cancel_url: String::new(),
    })
}

pub fn status_of(err: catalyrst_credits::http::ApiError) -> u16 {
    use axum::response::IntoResponse;
    err.into_response().status().as_u16()
}

/// Per-test SQL recorder used by `query_counts.rs` to assert how many DB round
/// trips a call path makes.
///
/// sqlx logs each executed statement as a DEBUG event on a target starting with
/// `sqlx` (`sqlx::query`), carrying the SQL in the `db.statement` field and a
/// short form in `summary`. We capture the rendered field text of each such
/// event so `count()` / `count_containing()` can be asserted.
///
/// One process-global subscriber routes events to the sink registered for the
/// emitting thread; tests MUST run on the current-thread flavor
/// (`#[tokio::test]`, default) so the pool's query futures are polled on the
/// thread that registered the sink. A scoped `set_default` per test would race
/// under parallel tests: every install/drop rebuilds tracing's process-global
/// callsite interest cache, and a rebuild issued from a thread whose default
/// has already reverted to the no-op subscriber stamps `Interest::never` onto
/// the sqlx callsites while another test's capture is mid-flight, so that test
/// records zero statements. The global subscriber is installed once and the
/// interest cache is rebuilt once (sqlx callsites that fired before install
/// hold a cached "never" from the no-op default), then never churned again.
struct RoutingSubscriber;

type SqlSink = Arc<Mutex<Vec<String>>>;
type SqlSinkMap = Mutex<HashMap<ThreadId, SqlSink>>;

fn sql_sinks() -> &'static SqlSinkMap {
    static SINKS: OnceLock<SqlSinkMap> = OnceLock::new();
    SINKS.get_or_init(Default::default)
}

impl tracing::Subscriber for RoutingSubscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() == tracing::Level::DEBUG && metadata.target().starts_with("sqlx")
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let sink = sql_sinks()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&std::thread::current().id())
            .cloned();
        if let Some(sink) = sink {
            let mut collector = FieldCollector(String::new());
            event.record(&mut collector);
            sink.lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(collector.0);
        }
    }

    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

struct FieldCollector(String);

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(self.0, " {}={:?}", field.name(), value);
    }
}

pub struct SqlCapture {
    events: SqlSink,
}

impl SqlCapture {
    /// Total sqlx query events captured while this guard was alive.
    pub fn count(&self) -> usize {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// How many captured statements contain `needle` (matched against the
    /// rendered field text, which includes `db.statement`).
    pub fn count_containing(&self, needle: &str) -> usize {
        self.events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|s| s.contains(needle))
            .count()
    }
}

impl Drop for SqlCapture {
    fn drop(&mut self) {
        sql_sinks()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&std::thread::current().id());
    }
}

/// Register an SQL-counting sink for the current thread. Drop the returned
/// guard to stop capturing.
pub fn sql_capture() -> SqlCapture {
    static INSTALL: OnceLock<()> = OnceLock::new();
    INSTALL.get_or_init(|| {
        tracing::subscriber::set_global_default(RoutingSubscriber)
            .expect("sql_capture owns the global subscriber in this test binary");
        tracing::callsite::rebuild_interest_cache();
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    sql_sinks()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(std::thread::current().id(), events.clone());
    SqlCapture { events }
}
