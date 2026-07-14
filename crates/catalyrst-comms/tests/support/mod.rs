//! Shared test-support for the catalyrst-comms optimization perf tests.
//!
//! ## SQL statement counting
//!
//! [`install_sql_counter`] installs a thread-local [`tracing`] subscriber that
//! records every `sqlx::query` completion event. sqlx 0.9 emits exactly one
//! such event (target `"sqlx::query"`, DEBUG) per executed statement from
//! `QueryLogger::finish`, giving a deterministic in-process statement counter
//! with no new dependencies.
//!
//! **Hard constraint:** `tracing::subscriber::set_default` is *thread-local*, so
//! every counting test MUST use a plain `#[tokio::test]` (current-thread
//! flavor -- never `flavor = "multi_thread"`). sqlx's own runtime is also
//! current-thread, so all query-completion events fire on the test thread and
//! are captured by this thread's scoped subscriber.
//!
//! `set_default` does not rebuild tracing's interest cache, and any sqlx query
//! run during seeding (before the subscriber is installed) registers the
//! `sqlx::query` callsites as `never`. `install_sql_counter` therefore calls
//! `tracing::callsite::rebuild_interest_cache()` so those callsites are
//! re-evaluated against the scoped subscriber -- without it the counter would
//! observe zero events.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use catalyrst_crypto::{create_simple_auth_chain, Wallet};
use sqlx::PgPool;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// A [`tracing_subscriber::Layer`] that records the concatenated field text of
/// every `sqlx::query` event, so tests can count how many statements a code
/// path executed.
#[derive(Clone, Default)]
pub struct SqlCounter {
    events: Arc<Mutex<Vec<String>>>,
}

impl SqlCounter {
    /// Number of recorded statements whose captured text contains `needle`.
    pub fn count_containing(&self, needle: &str) -> usize {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.contains(needle))
            .count()
    }

    /// Drop everything recorded so far (call after seeding, before the window
    /// under test).
    pub fn reset(&self) {
        self.events.lock().unwrap().clear();
    }
}

/// Concatenates all field values of an event into one searchable string. sqlx
/// splits the SQL across the `summary` (first 4 words) and `db.statement`
/// (full text, only when longer than the summary) fields; recording both means
/// the full statement is always searchable.
struct FieldCollector(String);

impl Visit for FieldCollector {
    fn record_str(&mut self, _field: &Field, value: &str) {
        self.0.push_str(value);
        self.0.push(' ');
    }

    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{value:?} ");
    }
}

impl<S: tracing::Subscriber> Layer<S> for SqlCounter {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut collector = FieldCollector(String::new());
        event.record(&mut collector);
        self.events.lock().unwrap().push(collector.0);
    }
}

/// Install a thread-local SQL statement counter. Keep the returned guard alive
/// for the duration of the counting window. See the module docs for the
/// current-thread-flavor constraint.
pub fn install_sql_counter() -> (SqlCounter, tracing::subscriber::DefaultGuard) {
    let counter = SqlCounter::default();
    let subscriber = tracing_subscriber::registry().with(counter.clone());
    let guard = tracing::subscriber::set_default(subscriber);
    // Re-evaluate the sqlx callsites (possibly cached `never` during seeding)
    // against the scoped subscriber we just installed.
    tracing::callsite::rebuild_interest_cache();
    (counter, guard)
}

/// An [`AppState`] over the given pools. Mirrors the literal in
/// `tests/submit_commit_epoch_author.rs`, adding the three knobs the batching
/// tests vary: `places_pool`, `dapps_pool`, and `dapps_schema`.
pub fn test_state(
    pool: PgPool,
    places_pool: Option<PgPool>,
    dapps_pool: Option<PgPool>,
    dapps_schema: impl Into<String>,
) -> AppState {
    let dapps_schema = dapps_schema.into();
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        player_reports: PlayerReportsComponent::new(pool.clone()),
        names: NamesComponent::new(dapps_pool.clone(), dapps_schema.clone()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool,
        dapps_pool,
        dapps_schema,
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: "http://127.0.0.1:1".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_host: "livekit.local".into(),
        livekit_ws_url: "wss://livekit.local".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: true,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
        fed_peer_id: "test-peer".into(),
    })
}

/// Signed-fetch headers for `method`+`path` (metadata `{}`), copied from
/// `tests/submit_commit_epoch_author.rs`.
pub fn signed_headers(wallet: &Wallet, method: &str, path: &str) -> HeaderMap {
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    let payload = build_payload(method, path, &timestamp, "{}");
    let chain = create_simple_auth_chain(wallet, &payload).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTH_TIMESTAMP_HEADER,
        HeaderValue::from_str(&timestamp).unwrap(),
    );
    headers.insert(AUTH_METADATA_HEADER, HeaderValue::from_static("{}"));
    for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
        headers.insert(
            HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes()).unwrap(),
            HeaderValue::from_str(&link.to_string()).unwrap(),
        );
    }
    headers
}
