//! The four federation routes, driven through the real axum router.
//!
//! The two assertions this file exists for:
//!
//! 1. `POST /admin/federation/worlds/refresh` authenticates its caller **before** it
//!    contacts anybody. Asserted not by reading the code but by counting inbound
//!    requests at the peer: an unauthenticated call must leave that counter at zero.
//!    This is the exact shape of the confused deputy that was caught before merge --
//!    a route holding privileged outbound reach that does not authenticate its own
//!    caller -- and the federated version is worse because it crosses a trust boundary.
//! 2. The mirror response contains no `owner` key, and none of the other authority
//!    words either, against the wire bytes rather than against the struct definition.
//!
//! Skips are announced on stderr by [`skipped`]; a pass tally from this file is not
//! evidence that anything ran.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{header, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_fed::PeerCert;
use catalyrst_worlds::config::Config;
use catalyrst_worlds::fed::config::WorldsFedConfig;
use catalyrst_worlds::fed::peers::{AdmissionOutcome, AdmittedPeer, WorldsFederationPeers};
use catalyrst_worlds::fed::poll::WorldsMirror;
use catalyrst_worlds::ports::bans::BansComponent;
use catalyrst_worlds::ports::denylist::DenyListComponent;
use catalyrst_worlds::ports::name_denylist::NameDenyListChecker;
use catalyrst_worlds::ports::presence::PeersRegistry;
use catalyrst_worlds::ports::worlds::WorldsComponent;
use catalyrst_worlds::rate_limiter::RateLimiter;
use catalyrst_worlds::{api_router, AppState, AppStateInner};
use serde_json::{json, Value};
use tower::ServiceExt;

const ADMIN_TOKEN: &str = "fed-routes-admin";
const PEER_A: &str = "peer-a.dclone.org";

fn skipped(property: &str) {
    eprintln!(
        "SKIPPED (no CATALYRST_WORLDS_TEST_PG): unverified \u{2014} {property}. \
         This test did not run; do not read its result as a pass."
    );
}

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds_fed_routes").await?;
    for sql in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0002_access_log.sql"),
        include_str!("../migrations/0003_permission_parcels.sql"),
        include_str!("../migrations/0004_lower_name_indexes.sql"),
        include_str!("../migrations/0005_federation_remote_worlds.sql"),
        include_str!("../migrations/0006_federation_deadmission.sql"),
        include_str!("../migrations/0010_world_settings_version.sql"),
        include_str!("../migrations/0012_world_realm_name_override.sql"),
        include_str!("../migrations/0013_world_preview_wearables.sql"),
    ] {
        scratch.apply_sql(sql).await;
    }
    Some(scratch)
}

// --- the stub peer, counting every inbound request -------------------------

#[derive(Clone)]
struct StubState {
    body: String,
    hits: Arc<AtomicUsize>,
}

async fn stub_worlds(AxumState(state): AxumState<StubState>) -> Response {
    state.hits.fetch_add(1, Ordering::SeqCst);
    (
        [(header::CONTENT_TYPE, "application/json")],
        state.body.clone(),
    )
        .into_response()
}

/// Counts **every** inbound request, not just `/worlds`, so a call that reaches the
/// peer at some other path is caught too.
async fn stub_any(AxumState(state): AxumState<StubState>) -> Response {
    state.hits.fetch_add(1, Ordering::SeqCst);
    StatusCode::NOT_FOUND.into_response()
}

struct StubPeer {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
}

impl StubPeer {
    async fn start(body: String) -> Self {
        let hits = Arc::new(AtomicUsize::new(0));
        let state = StubState {
            body,
            hits: hits.clone(),
        };
        let app = Router::new()
            .route("/worlds", get(stub_worlds))
            .fallback(get(stub_any))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self { addr, hits }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

// --- the app under test -----------------------------------------------------

fn fed_config() -> WorldsFedConfig {
    WorldsFedConfig {
        peers_file: Some(std::path::PathBuf::from("/nonexistent/peers.toml")),
        poll_interval_secs: 300,
        max_response_bytes: 4 * 1024 * 1024,
        max_worlds_per_peer: 10_000,
        allow_insecure_loopback_peers: true,
    }
}

fn admit(worlds_url: &str, cfg: &WorldsFedConfig) -> AdmittedPeer {
    let cert = PeerCert {
        version: 1,
        peer_id: PEER_A.to_string(),
        catalyst_url: "https://peer.dclone.org".to_string(),
        gossip_pubkey: [7u8; 32],
        mtls_root_pem: String::new(),
        dao_proposal: "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc123".to_string(),
        added_at: "2026-07-01".to_string(),
        worlds_url: worlds_url.to_string(),
    };
    match AdmittedPeer::admit(&cert, cfg) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("fixture peer must be admitted, got {other:?}"),
    }
}

fn test_config(federation: WorldsFedConfig) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 5146,
        database_url: "unused".into(),
        http_base_url: "http://fed.test".into(),
        network_id: 1,
        squid_database_url: None,
        global_scenes_urn: None,
        content_public_url: "http://fed.test/content".into(),
        lambdas_public_url: "http://fed.test/lambdas".into(),
        livekit_host: "livekit.fed.test".into(),
        livekit_ws_url: "wss://livekit.fed.test".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_configured: true,
        livekit_webhook_key: None,
        max_users_per_world: 100,
        comms_offline_when_unreachable: true,
        realm_name_strip_ens: true,
        preview_wearable_urns: Vec::new(),
        contents_upstream_url: None,
        contents_dir: std::env::temp_dir().join("catalyrst-fed-routes-contents"),
        comms_gatekeeper_url: None,
        comms_gatekeeper_auth_token: None,
        denylist_json_url: None,
        dcl_lists_url: None,
        admin_token: Some(ADMIN_TOKEN.into()),
        max_in_flight_upload_bytes: 512 * 1024 * 1024,
        max_concurrent_uploads: catalyrst_worlds::upload_limits::DEFAULT_MAX_CONCURRENT_UPLOADS,
        max_in_flight_upload_files:
            catalyrst_worlds::upload_limits::DEFAULT_MAX_IN_FLIGHT_UPLOAD_FILES,
        multipart_upload_timeout_ms:
            catalyrst_worlds::upload_limits::DEFAULT_MULTIPART_UPLOAD_TIMEOUT_MS,
        deployment_processing_timeout_ms:
            catalyrst_worlds::upload_limits::DEFAULT_DEPLOYMENT_PROCESSING_TIMEOUT_MS,
        federation,
    }
}

fn build_app(pool: sqlx::PgPool, peers: WorldsFederationPeers) -> (Router, AppState) {
    let cfg = fed_config();
    let http = reqwest::Client::builder().build().unwrap();
    let state: AppState = Arc::new(AppStateInner {
        sfu: catalyrst_livekit::SfuHealth::always_alive(),
        cfg: test_config(cfg.clone()),
        worlds: WorldsComponent::new(pool.clone()),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(),
        bans: BansComponent::new(http.clone(), None, None),
        denylist: DenyListComponent::new(http.clone(), None),
        name_denylist: NameDenyListChecker::new(http.clone(), None),
        http,
        squid_pool: None,
        mirror: WorldsMirror::new(pool, cfg, &peers),
        fed_peers: peers,
    });
    (api_router().with_state(state.clone()), state)
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.expect("router responds");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn get_req(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

fn listing(names: &[&str]) -> String {
    let worlds: Vec<_> = names
        .iter()
        .map(|n| {
            json!({
                "name": n,
                "title": format!("{n} title"),
                // The ownership claim a real peer sends on every entry.
                "owner": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "deployed_scenes": 2
            })
        })
        .collect();
    json!({ "worlds": worlds, "total": worlds.len() }).to_string()
}

/// **The confused-deputy assertion.** An unauthenticated refresh is refused, and the
/// peer never sees a request -- proven by the peer's own counter, not by reading the
/// handler.
#[tokio::test]
async fn an_unauthenticated_refresh_is_refused_before_any_peer_is_contacted() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "an unauthenticated POST to the refresh route reaches zero peers \u{2014} the \
             federated confused deputy",
        );
    };
    let stub = StubPeer::start(listing(&["a.dcl.eth"])).await;
    let cfg = fed_config();
    let peer = admit(&stub.url(), &cfg);
    let peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers: vec![peer],
        omitted: Vec::new(),
    };
    let (app, _state) = build_app(scratch.pool.clone(), peers);

    for headers in [
        None,
        Some("Bearer wrong"),
        Some("Basic anything"),
        Some(ADMIN_TOKEN),
    ] {
        let mut req = Request::builder()
            .method("POST")
            .uri("/admin/federation/worlds/refresh");
        if let Some(h) = headers {
            req = req.header("authorization", h);
        }
        let (status, _) = call(&app, req.body(Body::empty()).unwrap()).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "unauthenticated refresh must be refused (auth header: {headers:?})"
        );
        assert_eq!(
            stub.hits(),
            0,
            "the peer was contacted by an unauthenticated caller (auth header: {headers:?})"
        );
    }

    // The same route, authenticated, does reach the peer -- otherwise the assertion
    // above would be satisfied by a route that never works.
    let (status, body) = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/admin/federation/worlds/refresh")
            .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["polled"][0]["ok"], json!(true));
    assert_eq!(body["polled"][0]["worldsObserved"], json!(1));
    assert_eq!(
        stub.hits(),
        1,
        "the authenticated call is the only one that got through"
    );

    scratch.drop().await;
}

/// The veto route is the same shape and gets the same treatment.
#[tokio::test]
async fn the_veto_route_authenticates_before_it_reads_anything() {
    let Some(scratch) = setup_db().await else {
        return skipped("the hidden-veto route refuses an unauthenticated caller");
    };
    let stub = StubPeer::start(listing(&["a.dcl.eth"])).await;
    let cfg = fed_config();
    let peer = admit(&stub.url(), &cfg);
    let peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers: vec![peer],
        omitted: Vec::new(),
    };
    let (app, _state) = build_app(scratch.pool.clone(), peers);

    let (status, _) = call(
        &app,
        Request::builder()
            .method("PUT")
            .uri(format!(
                "/admin/federation/worlds/{PEER_A}/a.dcl.eth/hidden"
            ))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"hidden":true}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(stub.hits(), 0);

    scratch.drop().await;
}

/// The published mirror carries the peer's world and none of the peer's claims.
#[tokio::test]
async fn the_published_mirror_is_peer_qualified_and_carries_no_ownership_key() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "the mirror response is qualified by peerId and contains no owner/access/\
             permissions key",
        );
    };
    let stub = StubPeer::start(listing(&["remote-one.dcl.eth", "remote-two.dcl.eth"])).await;
    let cfg = fed_config();
    let peer = admit(&stub.url(), &cfg);
    let peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers: vec![peer],
        omitted: Vec::new(),
    };
    let (app, state) = build_app(scratch.pool.clone(), peers);

    // Poll through the authenticated admin route, as an operator would.
    let (status, _) = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/admin/federation/worlds/refresh")
            .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(&app, get_req("/federation/worlds/mirror")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], json!(2));
    for w in body["worlds"].as_array().expect("worlds array") {
        assert_eq!(
            w["peerId"],
            json!(PEER_A),
            "every row names the peer that said it"
        );
        assert!(w.get("owner").is_none());
    }

    let rendered = body.to_string();
    for forbidden in [
        "owner",
        "access",
        "permission",
        "blocked",
        "deployer",
        "singlePlayer",
        "0xdeadbeef",
    ] {
        assert!(
            !rendered.to_lowercase().contains(&forbidden.to_lowercase()),
            "the mirror response leaked {forbidden:?}:\n{rendered}"
        );
    }

    // The listing comes with peer health, so `worlds: []` can never be read without
    // also being told whether anyone answered.
    assert_eq!(body["peers"][0]["peerId"], json!(PEER_A));
    assert_eq!(body["peers"][0]["status"]["hasEverSucceeded"], json!(true));

    // And none of it reached the local surfaces.
    let (status, local) = call(&app, get_req("/worlds")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        local["total"],
        json!(0),
        "a mirrored world must not appear in /worlds"
    );
    let (status, _) = call(&app, get_req("/world/remote-one.dcl.eth/about")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "this server must not vouch for a peer's world under its own origin"
    );

    // /worlds/{name}/comms is the one that matters most: a mirrored row must not be
    // able to mint a LiveKit token in our cluster. There is no `worlds` row, so the
    // handler 404s before it ever reaches the access check.
    let (status, _) = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/worlds/remote-one.dcl.eth/comms")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a mirrored world must never mint a comms token"
    );

    drop(state);
    scratch.drop().await;
}

/// **Finding F, at the layer the operator actually reads.** A collision probe that
/// could not run must not reach the refresh JSON as an empty collision list.
///
/// `local_names_also_claimed` errors were swallowed into `Vec::new()` in
/// `fed/poll.rs`, and `refresh_federation_mirror` rendered that as
/// `localNameCollisions: []` -- the identical bytes a server with no collisions
/// produces. Same defect class as the zod laundering in `catalyrst/sites/packages/data/src/lib/catalyst/wcs.ts`: a failure
/// rendering as a measurement.
///
/// Both states are exercised in one test, against one route, so the assertion is that
/// they *differ* rather than that either looks a particular way in isolation.
#[tokio::test]
async fn a_refresh_reports_an_unavailable_collision_probe_as_null_not_as_an_empty_list() {
    let Some(scratch) = setup_db().await else {
        return skipped("the refresh JSON distinguishes `no collisions` from `we could not check`");
    };
    let stub = StubPeer::start(listing(&["probe-a.dcl.eth"])).await;
    let cfg = fed_config();
    let peer = admit(&stub.url(), &cfg);
    let peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers: vec![peer],
        omitted: Vec::new(),
    };
    let (app, _state) = build_app(scratch.pool.clone(), peers);

    fn refresh_req() -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/admin/federation/worlds/refresh")
            .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
            .body(Body::empty())
            .unwrap()
    }

    // Knowledge of an absence: the probe ran, and there is nothing to report.
    let (status, body) = call(&app, refresh_req()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["polled"][0]["ok"], json!(true));
    assert_eq!(
        body["polled"][0]["localNameCollisions"],
        json!([]),
        "a probe that ran and found nothing reports an empty list"
    );
    assert_eq!(body["polled"][0]["localNameCollisionsError"], json!(null));

    // Break the one table the probe reads. The mirror write does not touch it, so the
    // poll still succeeds -- which is the whole difficulty: `ok: true` alongside one
    // thing that was not checked.
    sqlx::query("DROP TABLE worlds CASCADE")
        .execute(&scratch.pool)
        .await
        .expect("drop the local worlds table");

    let (status, body) = call(&app, refresh_req()).await;
    assert_eq!(status, StatusCode::OK);
    let result = &body["polled"][0];
    assert_eq!(
        result["ok"],
        json!(true),
        "the fetch and the write succeeded; the probe decides nothing and does not \
         fail the poll"
    );
    assert_eq!(
        result["worldsObserved"],
        json!(1),
        "and the rows are real, so the count is a real count"
    );
    assert_eq!(
        result["localNameCollisions"],
        json!(null),
        "an unavailable probe must be absent, never an empty list: {result}"
    );
    assert!(
        result["localNameCollisionsError"]
            .as_str()
            .is_some_and(|e| !e.is_empty()),
        "and it must say why: {result}"
    );

    // The same absence is visible on the public health block, without an admin token:
    // fresh rows, and a recorded note that one thing about them went unchecked.
    let (status, peers_body) = call(&app, get_req("/federation/worlds/peers")).await;
    assert_eq!(status, StatusCode::OK);
    let status_block = &peers_body["peers"][0]["status"];
    assert_eq!(status_block["hasEverSucceeded"], json!(true));
    let last_error = status_block["lastError"]
        .as_str()
        .expect("the unavailable probe is recorded, not only logged");
    assert!(
        last_error.starts_with(catalyrst_worlds::fed::poll::COLLISION_PROBE_UNAVAILABLE_PREFIX),
        "the stored note must say which half failed: {last_error:?}"
    );

    scratch.drop().await;
}

/// The peers route reports the allowlist and never the secrets in it.
#[tokio::test]
async fn the_peers_route_reports_the_allowlist_without_its_secrets() {
    let Some(scratch) = setup_db().await else {
        return skipped("the peers route omits mtls_root_pem and gossip_pubkey");
    };
    let stub = StubPeer::start(listing(&["a.dcl.eth"])).await;
    let cfg = fed_config();
    let peer = admit(&stub.url(), &cfg);
    let peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers: vec![peer],
        omitted: Vec::new(),
    };
    let (app, _state) = build_app(scratch.pool.clone(), peers);

    let (status, body) = call(&app, get_req("/federation/worlds/peers")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["configured"], json!(true));
    assert_eq!(body["peers"][0]["peerId"], json!(PEER_A));
    assert_eq!(
        body["peers"][0]["status"]["hasEverSucceeded"],
        json!(false),
        "before any poll, a peer is explicitly 'never succeeded' rather than 'empty'"
    );
    assert_eq!(body["peers"][0]["insecureLoopback"], json!(true));

    let rendered = body.to_string().to_lowercase();
    for secret in ["mtls", "pem", "gossip", "pubkey", "-----begin"] {
        assert!(
            !rendered.contains(secret),
            "the peers route leaked {secret:?}:\n{rendered}"
        );
    }
    scratch.drop().await;
}

/// Unconfigured is a 503 naming the variable, on every route -- never an empty list.
#[tokio::test]
async fn every_federation_route_answers_503_when_federation_is_not_configured() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "all four federation routes answer 503 naming WORLDS_FED_PEERS_FILE when \
             unconfigured, rather than an empty list",
        );
    };
    let (app, _state) = build_app(scratch.pool.clone(), WorldsFederationPeers::NotConfigured);

    for (method, uri, body) in [
        ("GET", "/federation/worlds/peers", None),
        ("GET", "/federation/worlds/mirror", None),
        (
            "POST",
            "/admin/federation/worlds/refresh",
            Some(String::new()),
        ),
        (
            "PUT",
            "/admin/federation/worlds/peer-a.dclone.org/x.dcl.eth/hidden",
            Some(r#"{"hidden":true}"#.to_string()),
        ),
    ] {
        let mut req = Request::builder().method(method).uri(uri);
        // The admin routes authenticate FIRST, so they only reach the 503 with a
        // valid bearer. That ordering is the point: an unauthenticated caller learns
        // nothing about our configuration.
        if uri.starts_with("/admin/") {
            req = req
                .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
                .header("content-type", "application/json");
        }
        let (status, payload) = call(
            &app,
            req.body(body.map(Body::from).unwrap_or_else(Body::empty))
                .unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {uri} must 503 when unconfigured, not return an empty list"
        );
        assert!(
            payload.to_string().contains("WORLDS_FED_PEERS_FILE"),
            "{method} {uri} must name the variable that is unset: {payload}"
        );
    }
    scratch.drop().await;
}

/// An unknown peer id is a 404, not an empty listing that reads as a healthy peer
/// holding nothing.
#[tokio::test]
async fn an_unknown_peer_filter_is_a_404_not_an_empty_listing() {
    let Some(scratch) = setup_db().await else {
        return skipped("?peer= for a non-admitted id is a 404, not an empty list");
    };
    let stub = StubPeer::start(listing(&["a.dcl.eth"])).await;
    let cfg = fed_config();
    let peer = admit(&stub.url(), &cfg);
    let peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers: vec![peer],
        omitted: Vec::new(),
    };
    let (app, _state) = build_app(scratch.pool.clone(), peers);

    let (status, _) = call(
        &app,
        get_req("/federation/worlds/mirror?peer=someone-else.example.org"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = call(
        &app,
        get_req(&format!("/federation/worlds/mirror?peer={PEER_A}")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], json!(0), "admitted but not yet polled");
    assert_eq!(
        body["peers"][0]["status"]["hasEverSucceeded"],
        json!(false),
        "and the response says so, so the zero is not read as 'this peer is empty'"
    );
    scratch.drop().await;
}
