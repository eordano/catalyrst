//! The poll path, driven end to end against a real HTTP peer and a real database.
//!
//! Every test here points a genuinely admitted peer at a stub server we control and
//! asserts what the mirror did with what came back. The stubs lie in the ways a
//! compromised or merely broken peer would: they claim ownership, they name worlds
//! `../../etc/passwd`, they send ten megabytes, they answer 500, they go away
//! mid-run, and they publish a name we hold locally.
//!
//! **Reporting discipline.** `ScratchSchema::create` returns `None` when
//! `CATALYRST_WORLDS_TEST_PG` is unset, and a fully skipped run of this file is
//! textually identical to a real one -- same "N passed", same "ok". Every test that
//! needs the database therefore calls [`skipped`] on the way out, which names on
//! stderr the property that went unverified. A pass tally from this file is not
//! evidence; the stderr is.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State as AxumState;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_fed::PeerCert;
use catalyrst_worlds::fed::config::WorldsFedConfig;
use catalyrst_worlds::fed::peers::{AdmissionOutcome, AdmittedPeer, WorldsFederationPeers};
use catalyrst_worlds::fed::poll::WorldsMirror;
use serde_json::json;

// Harness

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds_fed_poll").await?;
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

/// Say, on stderr, which property went unchecked. A skip is not a pass, and this is
/// the line that makes the difference legible in a log.
fn skipped(property: &str) {
    eprintln!(
        "SKIPPED (no CATALYRST_WORLDS_TEST_PG): unverified \u{2014} {property}. \
         This test did not run; do not read its result as a pass."
    );
}

/// What the stub peer answers with. Each variant is a way a real peer has failed or
/// could lie.
#[derive(Clone)]
enum PeerBehaviour {
    /// A well-formed listing, verbatim.
    Body(String),
    /// A well-formed listing, but only on the Nth request onward. Before that: 500.
    /// Used to prove that a failure retains prior rows and a later success replaces
    /// them.
    FailsThenServes {
        after: usize,
        body: String,
    },
    Status(u16),
    /// 200 with a non-JSON content type. The listing bytes are valid JSON, so this
    /// isolates the content-type gate from the parser.
    WrongContentType(String),
    /// A body far larger than the configured cap.
    Oversized(usize),
    NotJson,
}

#[derive(Clone)]
struct StubState {
    behaviour: PeerBehaviour,
    hits: Arc<AtomicUsize>,
}

async fn stub_worlds(AxumState(state): AxumState<StubState>) -> Response {
    let n = state.hits.fetch_add(1, Ordering::SeqCst);
    match &state.behaviour {
        PeerBehaviour::Body(b) => {
            ([(header::CONTENT_TYPE, "application/json")], b.clone()).into_response()
        }
        PeerBehaviour::FailsThenServes { after, body } => {
            if n < *after {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            } else {
                ([(header::CONTENT_TYPE, "application/json")], body.clone()).into_response()
            }
        }
        PeerBehaviour::Status(s) => StatusCode::from_u16(*s).unwrap().into_response(),
        PeerBehaviour::WrongContentType(b) => {
            ([(header::CONTENT_TYPE, "text/html")], b.clone()).into_response()
        }
        PeerBehaviour::Oversized(n_bytes) => {
            let filler = "x".repeat(*n_bytes);
            let body =
                json!({ "worlds": [{ "name": "big.dcl.eth", "title": filler }] }).to_string();
            ([(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
        PeerBehaviour::NotJson => (
            [(header::CONTENT_TYPE, "application/json")],
            "<html>nope</html>",
        )
            .into_response(),
    }
}

struct StubPeer {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl StubPeer {
    async fn start(behaviour: PeerBehaviour) -> Self {
        let hits = Arc::new(AtomicUsize::new(0));
        let state = StubState {
            behaviour,
            hits: hits.clone(),
        };
        let app = Router::new()
            .route("/worlds", get(stub_worlds))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        Self {
            addr,
            hits,
            shutdown: Some(tx),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn fed_config(max_response_bytes: u64) -> WorldsFedConfig {
    WorldsFedConfig {
        peers_file: Some(std::path::PathBuf::from("/nonexistent/peers.toml")),
        poll_interval_secs: 300,
        max_response_bytes,
        max_worlds_per_peer: 10_000,
        // Required: this crate serves plain HTTP with no local TLS terminator, so a
        // two-node functional test cannot otherwise run. It does NOT exercise the
        // pinning path -- tests/federation_peer_admission.rs covers that separately
        // against a self-signed root.
        allow_insecure_loopback_peers: true,
    }
}

/// A peer certificate that clears every admission gate except the pinned root, which
/// the loopback opt-out excuses.
fn loopback_cert(peer_id: &str, worlds_url: &str) -> PeerCert {
    PeerCert {
        version: 1,
        peer_id: peer_id.to_string(),
        catalyst_url: "https://peer.dclone.org".to_string(),
        gossip_pubkey: [7u8; 32],
        mtls_root_pem: String::new(),
        dao_proposal: "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc123".to_string(),
        added_at: "2026-07-01".to_string(),
        worlds_url: worlds_url.to_string(),
    }
}

fn admit(peer_id: &str, worlds_url: &str, cfg: &WorldsFedConfig) -> AdmittedPeer {
    match AdmittedPeer::admit(&loopback_cert(peer_id, worlds_url), cfg) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("fixture peer must be admitted, got {other:?}"),
    }
}

fn registry(peers: Vec<AdmittedPeer>) -> WorldsFederationPeers {
    WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/peers.toml"),
        peers,
        omitted: Vec::new(),
    }
}

fn listing(names: &[&str]) -> String {
    let worlds: Vec<_> = names
        .iter()
        .map(|n| json!({ "name": n, "title": format!("{n} title"), "deployed_scenes": 1 }))
        .collect();
    json!({ "worlds": worlds, "total": worlds.len() }).to_string()
}

async fn seed_local_world(pool: &sqlx::PgPool, name: &str, owner: &str) {
    sqlx::query("INSERT INTO worlds (name, owner) VALUES ($1, $2)")
        .bind(name)
        .bind(owner)
        .execute(pool)
        .await
        .expect("seed local world");
}

async fn mirrored_names(pool: &sqlx::PgPool, peer: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT world_name FROM remote_worlds WHERE peer_id = $1 ORDER BY world_name",
    )
    .bind(peer)
    .fetch_all(pool)
    .await
    .expect("read mirror")
}

const PEER_A: &str = "peer-a.dclone.org";
const PEER_B: &str = "peer-b.dclone.org";

// The collision path

/// The headline collision case: one ENS name, held locally and claimed by a peer.
///
/// Local wins, and it wins *structurally* rather than by a comparison somebody has to
/// remember to write. The local row keeps its owner, the mirrored row is a separate
/// row in a separate table, `/worlds` is unchanged, and the collision is reported so
/// an operator can see it happened.
#[tokio::test]
async fn a_name_held_locally_and_claimed_by_a_peer_resolves_local_wins_and_is_reported() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "a peer claiming a locally-held world name leaves the local owner intact and \
             is reported as a collision",
        );
    };
    let owner = "0x1111111111111111111111111111111111111111";
    seed_local_world(&scratch.pool, "collide.dcl.eth", owner).await;

    let stub = StubPeer::start(PeerBehaviour::Body(listing(&[
        "collide.dcl.eth",
        "peer-only.dcl.eth",
    ])))
    .await;
    let cfg = fed_config(4 * 1024 * 1024);
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

    let report = mirror.poll_peer(&peer).await.expect("poll succeeds");

    assert_eq!(report.worlds_observed, 2);
    assert_eq!(
        report.collisions.checked(),
        Some(&["collide.dcl.eth".to_string()][..]),
        "the collision must be reported, not silently absorbed"
    );
    assert_eq!(
        report.collisions.unavailable_reason(),
        None,
        "the probe ran, so there is no reason it could not"
    );

    // The LOCAL row is untouched: same owner, still exactly one row.
    let local_owner: Option<String> =
        sqlx::query_scalar("SELECT owner FROM worlds WHERE lower(name) = 'collide.dcl.eth'")
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert_eq!(local_owner.as_deref(), Some(owner));
    let local_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(local_rows, 1, "the mirror did not add a row to `worlds`");

    // The REMOTE row exists, separately, under the peer.
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec![
            "collide.dcl.eth".to_string(),
            "peer-only.dcl.eth".to_string()
        ]
    );

    // And the mirrored row carries no ownership claim, because there is no column.
    let has_owner_column: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'remote_worlds' AND column_name = 'owner')",
    )
    .fetch_one(&scratch.pool)
    .await
    .unwrap();
    assert!(!has_owner_column);

    scratch.drop().await;
}

/// The other half of the collision path: what a poll says when it **could not look**.
///
/// The probe reads the local `worlds` table after the mirror rows are already written,
/// and it decides nothing -- so its failure correctly does not fail the poll. It used to
/// be swallowed into `Vec::new()`, which is the identical value a clean probe produces
/// on a server with no collisions, and that empty list then travelled all the way to the
/// operator as "no collisions". A query failure rendered as a measurement.
///
/// Three layers are asserted here because the lie had to be stopped at all three:
/// the in-process report, the stored status row, and (in
/// `tests/federation_mirror_routes.rs`) the JSON.
///
/// The probe is broken by dropping the table it reads. That is heavy-handed on purpose:
/// it produces a genuine `sqlx::Error` from the real query rather than a stub of one,
/// and it leaves `remote_worlds` -- the table the poll writes -- untouched, so the poll
/// itself still succeeds exactly as it would in production.
#[tokio::test]
async fn a_probe_that_could_not_run_reports_unknown_rather_than_no_collisions() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "a failed local-name collision probe is reported as unknown, in the poll \
             report and in remote_peer_status, rather than as an empty collision list",
        );
    };
    let cfg = fed_config(4 * 1024 * 1024);
    let stub = StubPeer::start(PeerBehaviour::Body(listing(&["probe.dcl.eth"]))).await;
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

    // A clean poll first, so the two states can be compared rather than described.
    let clean = mirror.poll_peer(&peer).await.expect("the clean poll");
    assert_eq!(
        clean.collisions.checked(),
        Some(&[][..]),
        "knowledge of an absence: the probe ran and found nothing"
    );
    let (clean_success, clean_error): (Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
        sqlx::query_as(
            "SELECT last_success_at, last_error FROM remote_peer_status WHERE peer_id = $1",
        )
        .bind(PEER_A)
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert!(clean_success.is_some());
    assert_eq!(clean_error, None, "a clean poll records no error");

    // Now break the one table the probe reads.
    sqlx::query("DROP TABLE worlds CASCADE")
        .execute(&scratch.pool)
        .await
        .expect("drop the local worlds table");

    let report = mirror
        .poll_peer(&peer)
        .await
        .expect("a probe failure must not fail a poll that already wrote its rows");

    // 1. The report. Absence of knowledge, with the reason attached.
    assert_eq!(
        report.collisions.checked(),
        None,
        "a failed probe must not present as a checked, empty list"
    );
    let reason = report
        .collisions
        .unavailable_reason()
        .expect("an unavailable probe carries why");
    assert!(
        reason.to_lowercase().contains("worlds"),
        "the reason must name the failure, got {reason:?}"
    );
    assert!(
        report.collisions.to_string().starts_with("unknown ("),
        "the log rendering must not be greppable as a count: {}",
        report.collisions
    );

    // The poll itself still did its job: the rows are fresh.
    assert_eq!(report.worlds_observed, 1);
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["probe.dcl.eth".to_string()]
    );

    // 2. The stored status. `last_success_at` advances, because the fetch and the write
    //    succeeded -- and `last_error` is set anyway, because one thing about those rows
    //    went unchecked. Neither field alone can say that.
    let (success, last_error): (Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
        sqlx::query_as(
            "SELECT last_success_at, last_error FROM remote_peer_status WHERE peer_id = $1",
        )
        .bind(PEER_A)
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert!(
        success > clean_success,
        "the fetch and the write succeeded, so the mirror is current"
    );
    let stored = last_error.expect("the unavailable probe is stored, not only logged");
    assert!(
        stored.starts_with(catalyrst_worlds::fed::poll::COLLISION_PROBE_UNAVAILABLE_PREFIX),
        "the stored note must say which half failed, got {stored:?}"
    );

    scratch.drop().await;
}

/// Two peers claiming the same name are two rows. Neither displaces the other, and a
/// later poll of one leaves the other alone.
#[tokio::test]
async fn two_peers_claiming_one_name_are_two_rows_and_a_poll_touches_only_its_own() {
    let Some(scratch) = setup_db().await else {
        return skipped("a poll of one peer replaces only that peer's rows");
    };
    let cfg = fed_config(4 * 1024 * 1024);

    let stub_a = StubPeer::start(PeerBehaviour::Body(listing(&[
        "shared.dcl.eth",
        "a-only.dcl.eth",
    ])))
    .await;
    let stub_b = StubPeer::start(PeerBehaviour::Body(listing(&[
        "shared.dcl.eth",
        "b-only.dcl.eth",
    ])))
    .await;
    let peer_a = admit(PEER_A, &stub_a.url(), &cfg);
    let peer_b = admit(PEER_B, &stub_b.url(), &cfg);
    let mirror = WorldsMirror::new(
        scratch.pool.clone(),
        cfg,
        &registry(vec![peer_a.clone(), peer_b.clone()]),
    );

    mirror.poll_peer(&peer_a).await.expect("poll a");
    mirror.poll_peer(&peer_b).await.expect("poll b");

    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["a-only.dcl.eth".to_string(), "shared.dcl.eth".to_string()]
    );
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_B).await,
        vec!["b-only.dcl.eth".to_string(), "shared.dcl.eth".to_string()]
    );

    // Re-poll A with a shorter listing: A shrinks, B is untouched.
    let mut stub_a2 = StubPeer::start(PeerBehaviour::Body(listing(&["a-only.dcl.eth"]))).await;
    let cfg2 = fed_config(4 * 1024 * 1024);
    let peer_a2 = admit(PEER_A, &stub_a2.url(), &cfg2);
    let mirror2 = WorldsMirror::new(scratch.pool.clone(), cfg2, &registry(vec![peer_a2.clone()]));
    mirror2.poll_peer(&peer_a2).await.expect("re-poll a");

    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["a-only.dcl.eth".to_string()]
    );
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_B).await,
        vec!["b-only.dcl.eth".to_string(), "shared.dcl.eth".to_string()],
        "polling A must not disturb B"
    );
    stub_a2.stop();
    scratch.drop().await;
}

// The lying peer

/// One stub, every lie at once. Ownership claims are dropped because there is nowhere
/// for them to land; a name shaped like a path traversal is refused and counted; the
/// good rows survive both.
#[tokio::test]
async fn a_lying_peer_loses_its_claims_and_its_bad_rows_without_losing_its_good_ones() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "a payload carrying owner/access/permissions stores none of them, and a \
             badly-shaped name is skipped rather than costing the whole listing",
        );
    };
    let body = json!({
        "total": 4,
        "worlds": [
            { "name": "honest.dcl.eth",
              "owner": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
              "access": { "type": "unrestricted" },
              "permissions": { "deployment": { "type": "allow-list", "wallets": ["0xdead"] } },
              "blocked_since": null,
              "deployer": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef" },
            { "name": "../../etc/passwd" },
            { "name": "has spaces.dcl.eth" },
            { "name": "also-honest.dcl.eth" }
        ]
    })
    .to_string();

    let stub = StubPeer::start(PeerBehaviour::Body(body)).await;
    let cfg = fed_config(4 * 1024 * 1024);
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

    let report = mirror.poll_peer(&peer).await.expect("poll succeeds");
    assert_eq!(report.worlds_observed, 2);
    assert_eq!(
        report.entries_skipped, 2,
        "each refused name is counted, not silently dropped"
    );
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec![
            "also-honest.dcl.eth".to_string(),
            "honest.dcl.eth".to_string()
        ]
    );

    // Nothing anywhere in the mirrored row mentions the wallet the peer named.
    let dumped: String =
        sqlx::query_scalar("SELECT coalesce(string_agg(r::text, ' '), '') FROM remote_worlds r")
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert!(
        !dumped.to_lowercase().contains("0xdeadbeef"),
        "a peer-asserted wallet reached storage: {dumped}"
    );

    scratch.drop().await;
}

/// A peer listing the same name twice would otherwise abort the whole upsert
/// (`ON CONFLICT DO UPDATE cannot affect row a second time`). The duplicate is
/// counted and dropped; the listing still lands.
#[tokio::test]
async fn a_duplicate_name_in_one_listing_does_not_abort_the_poll() {
    let Some(scratch) = setup_db().await else {
        return skipped("a peer listing one name twice does not abort the transaction");
    };
    let body = listing(&["dup.dcl.eth", "other.dcl.eth", "DUP.dcl.eth"]);
    let stub = StubPeer::start(PeerBehaviour::Body(body)).await;
    let cfg = fed_config(4 * 1024 * 1024);
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

    let report = mirror.poll_peer(&peer).await.expect("poll succeeds");
    assert_eq!(report.worlds_observed, 2);
    assert_eq!(report.entries_skipped, 1);
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["dup.dcl.eth".to_string(), "other.dcl.eth".to_string()]
    );
    scratch.drop().await;
}

// Every failure mode: stale, never empty

/// The property this whole file exists for. For each way a peer can fail, the previous
/// rows survive and the peer is recorded as failed -- because an empty listing is
/// indistinguishable from "this peer holds no worlds", and the mirror must never make
/// those two look the same.
#[tokio::test]
async fn every_peer_failure_retains_the_previous_rows_and_is_recorded_as_stale() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "an unreachable peer, a 500, a non-JSON body, an oversized body and a \
             malformed body each retain prior rows instead of emptying the mirror",
        );
    };

    // A good poll first, so there is something to lose.
    let good = StubPeer::start(PeerBehaviour::Body(listing(&["kept.dcl.eth"]))).await;
    let cfg = fed_config(4 * 1024 * 1024);
    let peer = admit(PEER_A, &good.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));
    mirror.poll_peer(&peer).await.expect("the good poll");
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["kept.dcl.eth".to_string()]
    );
    let first_success: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_success_at FROM remote_peer_status WHERE peer_id = $1")
            .bind(PEER_A)
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert!(first_success.is_some());

    // Each failure mode, against the same peer id, with the good rows already stored.
    let cases: Vec<(&str, PeerBehaviour, &str)> = vec![
        (
            "HTTP 500",
            PeerBehaviour::Status(500),
            "peer answered HTTP 500",
        ),
        (
            "HTTP 404",
            PeerBehaviour::Status(404),
            "peer answered HTTP 404",
        ),
        (
            "non-JSON content type",
            PeerBehaviour::WrongContentType(listing(&["evil.dcl.eth"])),
            "expected JSON",
        ),
        (
            "JSON content type, non-JSON body",
            PeerBehaviour::NotJson,
            "not a worlds listing",
        ),
        (
            "body over the byte cap",
            PeerBehaviour::Oversized(300_000),
            "refused before parse",
        ),
    ];

    for (label, behaviour, expected_fragment) in cases {
        let mut stub = StubPeer::start(behaviour).await;
        // A deliberately small cap so the oversized case is cheap to produce.
        let cfg = fed_config(64 * 1024);
        let peer = admit(PEER_A, &stub.url(), &cfg);
        let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

        let err = mirror
            .poll_peer(&peer)
            .await
            .expect_err(&format!("{label} must fail the poll"));
        let msg = err.to_string();
        assert!(
            msg.contains(expected_fragment),
            "{label}: expected {expected_fragment:?} in {msg:?}"
        );

        assert_eq!(
            mirrored_names(&scratch.pool, PEER_A).await,
            vec!["kept.dcl.eth".to_string()],
            "{label} emptied the mirror; it must only make it stale"
        );

        let (success, last_error): (Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
            sqlx::query_as(
                "SELECT last_success_at, last_error FROM remote_peer_status WHERE peer_id = $1",
            )
            .bind(PEER_A)
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
        assert_eq!(
            success, first_success,
            "{label} must not advance last_success_at \u{2014} staleness is the signal"
        );
        assert!(
            last_error.is_some(),
            "{label} must be recorded so the staleness has a reason"
        );
        stub.stop();
    }

    // And an unreachable peer: the stub is gone entirely.
    let mut dead = StubPeer::start(PeerBehaviour::Body(listing(&["gone.dcl.eth"]))).await;
    let dead_url = dead.url();
    dead.stop();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let cfg = fed_config(4 * 1024 * 1024);
    let peer = admit(PEER_A, &dead_url, &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));
    let err = mirror
        .poll_peer(&peer)
        .await
        .expect_err("an unreachable peer must fail the poll");
    assert!(err.to_string().starts_with("transport:"), "{err}");
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["kept.dcl.eth".to_string()],
        "an unreachable peer must leave its last-good rows in place"
    );

    scratch.drop().await;
}

/// A peer that has never answered has no rows *and* no `last_success_at`. The two
/// facts together are what stop a consumer reading "no worlds" out of "no contact".
#[tokio::test]
async fn a_peer_that_never_answered_is_distinguishable_from_a_peer_with_no_worlds() {
    let Some(scratch) = setup_db().await else {
        return skipped(
            "never-contacted and genuinely-empty peers are distinguishable in \
             remote_peer_status",
        );
    };
    let cfg = fed_config(4 * 1024 * 1024);

    // Peer A: never answers.
    let mut dead = StubPeer::start(PeerBehaviour::Body(String::new())).await;
    let dead_url = dead.url();
    dead.stop();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let peer_a = admit(PEER_A, &dead_url, &cfg);

    // Peer B: answers, honestly, that it holds nothing.
    let empty = StubPeer::start(PeerBehaviour::Body(
        json!({ "worlds": [], "total": 0 }).to_string(),
    ))
    .await;
    let peer_b = admit(PEER_B, &empty.url(), &cfg);

    let mirror = WorldsMirror::new(
        scratch.pool.clone(),
        cfg,
        &registry(vec![peer_a.clone(), peer_b.clone()]),
    );
    let _ = mirror.poll_peer(&peer_a).await;
    mirror
        .poll_peer(&peer_b)
        .await
        .expect("the empty peer polls fine");

    // Both have zero rows...
    assert!(mirrored_names(&scratch.pool, PEER_A).await.is_empty());
    assert!(mirrored_names(&scratch.pool, PEER_B).await.is_empty());

    // ...and are not the same state.
    let rows: Vec<(
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT peer_id, last_success_at, last_error FROM remote_peer_status ORDER BY peer_id",
    )
    .fetch_all(&scratch.pool)
    .await
    .unwrap();
    let a = rows
        .iter()
        .find(|r| r.0 == PEER_A)
        .expect("peer a recorded");
    let b = rows
        .iter()
        .find(|r| r.0 == PEER_B)
        .expect("peer b recorded");
    assert!(a.1.is_none(), "a peer we never reached has no success time");
    assert!(a.2.is_some(), "and has a recorded reason");
    assert!(
        b.1.is_some(),
        "a peer that answered empty HAS a success time"
    );
    assert!(b.2.is_none(), "and no error");

    scratch.drop().await;
}

/// A failure followed by a success replaces the rows properly -- staleness is
/// recoverable, not a latch.
#[tokio::test]
async fn a_recovered_peer_replaces_its_stale_rows() {
    let Some(scratch) = setup_db().await else {
        return skipped("a peer that recovers replaces its stale rows and clears last_error");
    };
    let cfg = fed_config(4 * 1024 * 1024);
    let stub = StubPeer::start(PeerBehaviour::FailsThenServes {
        after: 1,
        body: listing(&["fresh.dcl.eth"]),
    })
    .await;
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

    mirror.poll_peer(&peer).await.expect_err("first poll fails");
    assert!(mirrored_names(&scratch.pool, PEER_A).await.is_empty());

    let report = mirror.poll_peer(&peer).await.expect("second poll succeeds");
    assert_eq!(report.worlds_observed, 1);
    assert_eq!(
        mirrored_names(&scratch.pool, PEER_A).await,
        vec!["fresh.dcl.eth".to_string()]
    );
    let last_error: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM remote_peer_status WHERE peer_id = $1")
            .bind(PEER_A)
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert_eq!(last_error, None, "a success clears the recorded failure");
    assert!(stub.hits() >= 2);
    scratch.drop().await;
}

// The local operator veto

/// `hidden_since` is ours. The poller's DELETE spares vetoed rows and its UPDATE arm
/// does not name the column, so a peer cannot un-hide itself by re-listing.
#[tokio::test]
async fn a_vetoed_row_survives_a_poll_and_stays_out_of_the_published_listing() {
    let Some(scratch) = setup_db().await else {
        return skipped("hidden_since survives a poll and excludes the row from the mirror");
    };
    let cfg = fed_config(4 * 1024 * 1024);
    let stub = StubPeer::start(PeerBehaviour::Body(listing(&[
        "hidden.dcl.eth",
        "shown.dcl.eth",
    ])))
    .await;
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let peers = registry(vec![peer.clone()]);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &peers);
    mirror.poll_peer(&peer).await.expect("first poll");

    // The operator-facing constructor: the same shape rules, and no way to reach
    // `resolve_world_owner` with the result.
    let name =
        catalyrst_worlds::fed::names::RemoteWorldName::from_operator_veto_path("hidden.dcl.eth")
            .expect("a plain name");
    assert!(
        mirror
            .store()
            .set_hidden(peer.peer_id(), &name, true)
            .await
            .expect("veto applies"),
        "the row exists, so the veto reports that it landed"
    );

    // Re-poll: the peer re-lists both, and cannot revoke the veto.
    mirror.poll_peer(&peer).await.expect("second poll");
    let still_hidden: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT hidden_since FROM remote_worlds WHERE peer_id = $1 AND world_name = $2",
    )
    .bind(PEER_A)
    .bind("hidden.dcl.eth")
    .fetch_one(&scratch.pool)
    .await
    .unwrap();
    assert!(
        still_hidden.is_some(),
        "a peer re-listing a world must not clear our veto"
    );

    let (published, total) = mirror
        .store()
        .list_mirror(&peers, Some(peer.peer_id()), 100, 0)
        .await
        .expect("list");
    assert_eq!(total, 1);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].name.as_peer_reported_str(), "shown.dcl.eth");

    // And the veto is reversible by us.
    assert!(mirror
        .store()
        .set_hidden(peer.peer_id(), &name, false)
        .await
        .unwrap());
    let (published, _) = mirror
        .store()
        .list_mirror(&peers, Some(peer.peer_id()), 100, 0)
        .await
        .unwrap();
    assert_eq!(published.len(), 2);

    scratch.drop().await;
}

// Caps

/// The row cap truncates and *says so*. A shorter list presented as complete would be
/// the same lie as an empty list presented as "no worlds".
#[tokio::test]
async fn the_row_cap_marks_the_peer_truncated_rather_than_silently_shortening() {
    let Some(scratch) = setup_db().await else {
        return skipped("the per-peer row cap records `truncated` rather than shortening quietly");
    };
    let names: Vec<String> = (0..50).map(|i| format!("w{i:03}.dcl.eth")).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let stub = StubPeer::start(PeerBehaviour::Body(listing(&refs))).await;

    let mut cfg = fed_config(4 * 1024 * 1024);
    cfg.max_worlds_per_peer = 10;
    let peer = admit(PEER_A, &stub.url(), &cfg);
    let mirror = WorldsMirror::new(scratch.pool.clone(), cfg, &registry(vec![peer.clone()]));

    let report = mirror.poll_peer(&peer).await.expect("poll succeeds");
    assert_eq!(report.worlds_observed, 10);
    assert!(report.truncated);

    let truncated: bool =
        sqlx::query_scalar("SELECT truncated FROM remote_peer_status WHERE peer_id = $1")
            .bind(PEER_A)
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert!(
        truncated,
        "the cap is visible on the wire, not only in a log"
    );
    assert_eq!(mirrored_names(&scratch.pool, PEER_A).await.len(), 10);
    scratch.drop().await;
}

/// The URL the poller builds comes from the registry and nothing else. A peer response
/// cannot steer an outbound request because no response value reaches a URL
/// constructor -- there is no parameter that could carry one.
#[tokio::test]
async fn the_listing_url_is_built_from_the_registry_and_hits_only_the_registered_host() {
    let cfg = fed_config(4 * 1024 * 1024);
    let stub = StubPeer::start(PeerBehaviour::Body(listing(&["x.dcl.eth"]))).await;
    let peer = admit(PEER_A, &stub.url(), &cfg);

    let url = peer.worlds_listing_url(500, 0);
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(url.port(), Some(stub.addr.port()));
    assert_eq!(url.path(), "/worlds");
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q.get("limit").map(String::as_str), Some("500"));
    assert_eq!(q.get("offset").map(String::as_str), Some("0"));
    assert_eq!(
        url.as_str().matches("//").count(),
        1,
        "the path is fixed in one place; nothing appends a peer-chosen segment"
    );
}
