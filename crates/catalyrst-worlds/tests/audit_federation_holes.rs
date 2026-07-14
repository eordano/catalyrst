//! ADVERSARIAL AUDIT of the worlds-federation read mirror.
//!
//! Each test here started as a demonstration of a hole the build left open. None of
//! them is a peer-to-authorization path -- that category came back clean -- but three of
//! them were places where the module's own stated contract did not hold.
//!
//! Written to be inverted, never deleted: when a follow-up closes a hole, the test
//! flips from "demonstrates the bug" to "asserts the fix" and the section header records
//! what the bug was, so the regression that reopens it fails here.
//!
//! Status:
//!   - HOLE 1, de-admission does not revoke publication -- **CLOSED**, inverted below.
//!   - HOLE 2, case-variant peer ids collapse into one namespace -- **CLOSED**, inverted.
//!   - HOLE 3, the veto route answers an anonymous caller before authorizing --
//!     **CLOSED**, inverted below.
//!   - HOLE 4, the provenance grep gate has a two-line blind spot -- **CLOSED**,
//!     inverted below.
//!   - HOLE 5, `insecureLoopback` is false for a cleartext http peer -- **CLOSED**,
//!     inverted below.
//!   - HOLE 6, a failed collision probe rendered as "no collisions" -- **CLOSED**; the
//!     test below is new rather than inverted, because the audit demonstrated this one
//!     against a live binary and left no test behind.
//!   - HOLE 7, a peer file that names nobody is accepted and sweeps the mirror --
//!     **CLOSED** at the sweep (not at the loader), inverted below; **introduced by
//!     HOLE 1's fix**. Three tests, all inverted.
//!   - HOLE 8, the collision probe's database errors reach the public peers route --
//!     **CLOSED**, inverted below; **introduced by HOLE 6's fix**.
//!
//! HOLES 7 and 8 are the re-audit's own findings: each closure was verified against a
//! live binary, and each opened something smaller on its way past. Both are now closed
//! too, and this file is entirely assertions of fixes -- which is the state in which it
//! is most worth keeping, because every one of them fails if the fix is undone.
//!
//! Two of the closures are worth reading before touching this module again, because the
//! obvious fix was the wrong one in both:
//!
//!   - HOLE 7 is guarded at the **sweep**, not at the loader. Refusing an empty peer
//!     file was tried first and it destroyed the one legitimate way to say "federation
//!     is on and we admit nobody" -- the exact distinction the two-variant enum exists
//!     to carry. The damage was never the empty set; it was the DELETE it drove.
//!   - HOLE 4 is closed by a **structural** rule, not a cleverer line rule. A two-line
//!     launder beat a one-line check; a three-line launder would have beaten a two-line
//!     one. Banning the local-name constructor everywhere under `src/fed/` ends the
//!     class instead of the instance.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_fed::PeerCert;
use catalyrst_worlds::config::Config;
use catalyrst_worlds::fed::config::WorldsFedConfig;
use catalyrst_worlds::fed::names::RemoteWorldName;
use catalyrst_worlds::fed::peers::{AdmissionOutcome, AdmittedPeer, WorldsFederationPeers};
use catalyrst_worlds::fed::poll::WorldsMirror;
use catalyrst_worlds::fed::store::{RemoteWorld, RemoteWorldsComponent, Revocation, SweptBecause};
use catalyrst_worlds::ports::bans::BansComponent;
use catalyrst_worlds::ports::denylist::DenyListComponent;
use catalyrst_worlds::ports::name_denylist::NameDenyListChecker;
use catalyrst_worlds::ports::presence::PeersRegistry;
use catalyrst_worlds::ports::worlds::WorldsComponent;
use catalyrst_worlds::rate_limiter::RateLimiter;
use catalyrst_worlds::{api_router, AppState, AppStateInner};
use serde_json::Value;
use tower::ServiceExt;

const ADMIN_TOKEN: &str = "audit-admin";

fn skipped(property: &str) {
    eprintln!(
        "SKIPPED-AUDIT (no CATALYRST_WORLDS_TEST_PG): unverified \u{2014} {property}. \
         Do not read this test's result as a pass."
    );
}

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds_fed_audit").await?;
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

fn fed_config() -> WorldsFedConfig {
    WorldsFedConfig {
        peers_file: Some(std::path::PathBuf::from("/nonexistent/peers.toml")),
        poll_interval_secs: 300,
        max_response_bytes: 4 * 1024 * 1024,
        max_worlds_per_peer: 10_000,
        allow_insecure_loopback_peers: true,
    }
}

fn cert(peer_id: &str, worlds_url: &str) -> PeerCert {
    PeerCert {
        version: 1,
        peer_id: peer_id.to_string(),
        catalyst_url: "https://peer.example.org/content".to_string(),
        gossip_pubkey: [7u8; 32],
        mtls_root_pem: String::new(),
        dao_proposal: "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc123".to_string(),
        added_at: "2026-07-01".to_string(),
        worlds_url: worlds_url.to_string(),
    }
}

fn admit(peer_id: &str, worlds_url: &str) -> AdmittedPeer {
    match AdmittedPeer::admit(&cert(peer_id, worlds_url), &fed_config()) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("fixture peer {peer_id} must be admitted, got {other:?}"),
    }
}

fn test_config(federation: WorldsFedConfig) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 5146,
        database_url: "unused".into(),
        http_base_url: "http://audit.test".into(),
        network_id: 1,
        squid_database_url: None,
        global_scenes_urn: None,
        content_public_url: "http://audit.test/content".into(),
        lambdas_public_url: "http://audit.test/lambdas".into(),
        livekit_host: "livekit.audit.test".into(),
        livekit_ws_url: "wss://livekit.audit.test".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_configured: true,
        livekit_webhook_key: None,
        max_users_per_world: 100,
        comms_offline_when_unreachable: true,
        realm_name_strip_ens: true,
        preview_wearable_urns: Vec::new(),
        contents_upstream_url: None,
        contents_dir: std::env::temp_dir().join("catalyrst-fed-audit-contents"),
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

fn build_app(pool: sqlx::PgPool, peers: WorldsFederationPeers) -> Router {
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
    api_router().with_state(state)
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.expect("router responds");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn row(peer: &catalyrst_worlds::fed::names::PeerId, name: &str) -> RemoteWorld {
    RemoteWorld {
        peer_id: peer.clone(),
        name: RemoteWorldName::from_operator_veto_path(name).expect("shaped name"),
        title: Some("mirrored".into()),
        description: None,
        content_rating: None,
        categories: None,
        thumbnail_hash: None,
        deployed_scenes: 1,
        last_deployed_at: None,
        observed_at: chrono::Utc::now(),
        hidden_since: None,
    }
}

// HOLE 1 -- de-admission does not revoke publication  [CLOSED]
//
// All three tests below are INVERTED: they asserted the bug, they now assert the fix.
//
// What the bug was. `remote_worlds` rows are written per `peer_id`, and nothing ever
// compared them to the admitted set. `list_mirror` filtered on `hidden_since` and an
// optional peer id and joined against nothing, and no boot path pruned. The spec's
// revocation mechanism is a restart (S2.5: "registry reload at runtime: not built ...
// changing it is a restart") and the restart revoked nothing: a peer the DAO had
// dropped kept every one of its worlds published under our origin, indefinitely, while
// `/federation/worlds/peers` no longer listed it and `?peer=` answered 404 for it. Its
// content was published with no way to attribute or interrogate it, and the `peers[]`
// health block carried no line for it -- so `hasEverSucceeded` and `lastSuccessAt`, the
// two fields whose entire job is to keep stale data honest, were absent for the most
// stale data we held.
//
// What the fix is. Two mechanisms, deliberately on two different paths:
//   - WRITE, at boot: `RemoteWorldsComponent::revoke_peers_no_longer_admitted` runs in
//     `build_state` after the migrations and before the `AppState` the router is built
//     from exists. It DELETEs the per-world rows of every peer not in the allowlist and
//     tombstones the peer in `remote_peer_status` (`deadmitted_at`,
//     `deadmitted_worlds_deleted`, migration 0006).
//   - READ, per request: `list_mirror` takes the allowlist and filters
//     `peer_id = ANY($admitted)`. `GET /federation/worlds/mirror` passes the same
//     `state.fed_peers` value it renders `peers[]` from, so a row and a status line come
//     from one value read twice in one request.
//
// Neither is allowed to be the only thing standing between a revoked peer and
// publication, which is why `a_revoked_peers_rows_are_unpublishable_even_if_the_boot_sweep_never_ran`
// exists: it skips the sweep entirely and asserts the route publishes nothing anyway.

/// **The fix.** A peer removed from `federation-peers.toml` is unlisted, unaddressable,
/// **and unpublished**, and the two federation routes agree about it.
///
/// The pre-fix version of this test asserted `worlds.len() == 2` here, with a comment
/// beginning "HOLE:". The three assertions that flipped are marked below.
#[tokio::test]
async fn a_revoked_peers_worlds_stop_being_published_and_the_two_routes_agree() {
    let Some(scratch) = setup_db().await else {
        skipped("removing a peer from the allowlist stops publishing its worlds");
        return;
    };
    let pool = scratch.pool.clone();

    // --- day 1: the peer is admitted and we mirror two of its worlds ---------
    let admitted = admit("revoked-peer.org", "http://127.0.0.1:1/");
    let store = RemoteWorldsComponent::new(pool.clone());
    store
        .replace_peer_worlds(
            admitted.peer_id(),
            &[
                row(admitted.peer_id(), "kept.dcl.eth"),
                row(admitted.peer_id(), "alsokept.dcl.eth"),
            ],
        )
        .await
        .expect("mirror the peer");
    store
        .record_success(admitted.peer_id(), 2, 0, false)
        .await
        .expect("record success");

    // A second peer that is still in the file, so the sweep has to be selective rather
    // than emptying the table.
    let survivor = admit("kept-peer.org", "http://127.0.0.1:2/");
    store
        .replace_peer_worlds(
            survivor.peer_id(),
            &[row(survivor.peer_id(), "survivor.dcl.eth")],
        )
        .await
        .expect("mirror the surviving peer");

    // --- day 2: the DAO revokes it. The operator deletes the entry and restarts. ---
    // That is exactly this state: configured, file loaded, this peer not in it.
    let after_revocation = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/etc/catalyrst/federation-peers.toml"),
        peers: vec![survivor.clone()],
        omitted: Vec::new(),
    };

    // The boot sweep. In production this is `build_state`, between the migrations and
    // the construction of the state the router is built from.
    let revocation = store
        .revoke_peers_no_longer_admitted(&after_revocation)
        .await
        .expect("the boot sweep runs");
    assert_eq!(
        revocation.worlds_deleted(),
        2,
        "both of the revoked peer's rows are destroyed, and only those two"
    );
    assert_eq!(revocation.revoked_peer_ids(), vec!["revoked-peer.org"]);

    let app = build_app(pool.clone(), after_revocation);

    // The allowlist no longer names it...
    let (s, peers) = call(
        &app,
        Request::builder()
            .uri("/federation/worlds/peers")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let listed: Vec<&str> = peers["peers"]
        .as_array()
        .expect("peers array")
        .iter()
        .map(|p| p["peerId"].as_str().expect("peerId"))
        .collect();
    assert_eq!(
        listed,
        vec!["kept-peer.org"],
        "the revoked peer must not be in the allowlist"
    );

    // ...it is not addressable...
    let (s, _) = call(
        &app,
        Request::builder()
            .uri("/federation/worlds/mirror?peer=revoked-peer.org")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "a revoked peer is not addressable by name"
    );

    // ...and, the assertion that flipped, we no longer publish its worlds.
    let (s, mirror) = call(
        &app,
        Request::builder()
            .uri("/federation/worlds/mirror")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let worlds = mirror["worlds"].as_array().expect("worlds array");
    let published: Vec<&str> = worlds
        .iter()
        .map(|w| w["peerId"].as_str().expect("peerId"))
        .collect();
    assert_eq!(
        published,
        vec!["kept-peer.org"],
        "a peer removed from the allowlist must not be published; only the peer still \
         in the file may contribute a row. Got: {mirror}"
    );
    assert_eq!(
        mirror["total"], 1,
        "`total` is filtered by the same predicate as `worlds`, so a client cannot be \
         told there is more behind the page than it may fetch"
    );

    // The disagreement the audit named is gone in both directions: every published row
    // has a status line, and every status line belongs to a listed peer.
    let status_lines: Vec<&str> = mirror["peers"]
        .as_array()
        .expect("peers block")
        .iter()
        .map(|p| p["peerId"].as_str().expect("peerId"))
        .collect();
    assert_eq!(status_lines, vec!["kept-peer.org"]);
    for peer_id in &published {
        assert!(
            status_lines.contains(peer_id),
            "a world was published for {peer_id:?} with no status line, so \
             `hasEverSucceeded` cannot be consulted for it"
        );
    }
    assert_eq!(
        status_lines, listed,
        "/federation/worlds/mirror's health block and /federation/worlds/peers name the \
         same peers, because both are rendered from one `state.fed_peers`"
    );

    // The bounded half of the audit trail. The per-world rows are gone; the row that
    // says we once published them, and how many there were, is not.
    let tomb: (
        Option<chrono::DateTime<chrono::Utc>>,
        i64,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT deadmitted_at, deadmitted_worlds_deleted, last_success_at \
             FROM remote_peer_status WHERE peer_id = 'revoked-peer.org'",
    )
    .fetch_one(&pool)
    .await
    .expect("the de-admitted peer still has a status row");
    assert!(
        tomb.0.is_some(),
        "DELETE loses the audit trail unless something records that it happened; \
         deadmitted_at is that something"
    );
    assert_eq!(
        tomb.1, 2,
        "and it records how much was destroyed, which is the part a bare DELETE loses"
    );
    assert!(
        tomb.2.is_some(),
        "along with when we last successfully heard from the peer, which was already here"
    );

    scratch.drop().await;
}

/// **Mechanism independence.** The boot sweep and the read filter are two mechanisms on
/// two paths, and the audit's point was that publication must not rest on either alone.
///
/// So this test deliberately **never calls the sweep**. It leaves the revoked peer's
/// rows sitting in `remote_worlds` -- which is also the real state during a rolling
/// deploy, when an old process still holding the peer in its allowlist keeps re-writing
/// them underneath a new process that does not -- and asserts that the route publishes
/// nothing regardless.
#[tokio::test]
async fn a_revoked_peers_rows_are_unpublishable_even_if_the_boot_sweep_never_ran() {
    let Some(scratch) = setup_db().await else {
        skipped("the read path refuses de-admitted rows without help from the boot sweep");
        return;
    };
    let pool = scratch.pool.clone();
    let store = RemoteWorldsComponent::new(pool.clone());

    let ghost = admit("ghost-peer.org", "http://127.0.0.1:1/");
    store
        .replace_peer_worlds(ghost.peer_id(), &[row(ghost.peer_id(), "ghost.dcl.eth")])
        .await
        .expect("mirror the peer");

    let after_revocation = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/etc/catalyrst/federation-peers.toml"),
        peers: Vec::new(),
        omitted: Vec::new(),
    };

    // The row is still physically there. Nothing has been pruned.
    let still_stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_worlds WHERE peer_id = 'ghost-peer.org'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(still_stored, 1, "the sweep was deliberately not run");

    let (rows, total) = store
        .list_mirror(&after_revocation, None, 100, 0)
        .await
        .expect("list");
    assert!(
        rows.is_empty() && total == 0,
        "a stored row for a de-admitted peer must not be publishable by the read path \
         even when the write path never pruned it"
    );

    let app = build_app(pool.clone(), after_revocation);
    let (s, mirror) = call(
        &app,
        Request::builder()
            .uri("/federation/worlds/mirror")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(mirror["worlds"].as_array().map(Vec::len), Some(0));
    assert_eq!(mirror["total"], 0);

    scratch.drop().await;
}

/// The two decisions inside the sweep that are easy to get wrong, asserted rather than
/// described.
///
/// 1. **An unset `WORLDS_FED_PEERS_FILE` is not a revocation.** There is no adjudicated
///    allowlist in that state, so there is nothing to enforce and nothing is destroyed --
///    but nothing is published either, because the routes answer 503 and `list_mirror`
///    filters against an empty admitted set. Unsetting an environment variable must not
///    be one keystroke away from destroying every mirrored row.
/// 2. **A local operator veto survives de-admission.** `hidden_since` is ours, not the
///    peer's; it took a deliberate admin action to record, and the row it marks is
///    published by nothing either way. Deleting it would mean a re-admitted peer
///    silently gets a world we vetoed published again.
#[tokio::test]
async fn the_sweep_spares_an_unconfigured_server_and_spares_a_local_veto() {
    let Some(scratch) = setup_db().await else {
        skipped("the sweep destroys nothing when there is no allowlist, and spares vetoed rows");
        return;
    };
    let pool = scratch.pool.clone();
    let store = RemoteWorldsComponent::new(pool.clone());

    let peer = admit("veto-peer.org", "http://127.0.0.1:1/");
    store
        .replace_peer_worlds(
            peer.peer_id(),
            &[
                row(peer.peer_id(), "vetoed.dcl.eth"),
                row(peer.peer_id(), "plain.dcl.eth"),
            ],
        )
        .await
        .expect("mirror the peer");
    let vetoed = RemoteWorldName::from_operator_veto_path("vetoed.dcl.eth").expect("shaped");
    assert!(store
        .set_hidden(peer.peer_id(), &vetoed, true)
        .await
        .expect("veto applies"));

    // --- 1. no allowlist to enforce: nothing is written -----------------------
    let unconfigured = store
        .revoke_peers_no_longer_admitted(&WorldsFederationPeers::NotConfigured)
        .await
        .expect("the sweep runs");
    assert_eq!(
        unconfigured.worlds_deleted(),
        0,
        "unsetting WORLDS_FED_PEERS_FILE is a configuration state, not a DAO revocation"
    );
    let survived: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_worlds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(survived, 2, "both rows are still there");

    // ...and nothing is publishable in that state either, so the retention costs
    // nothing: an empty admitted set matches no peer.
    let (rows, total) = store
        .list_mirror(&WorldsFederationPeers::NotConfigured, None, 100, 0)
        .await
        .expect("list");
    assert!(
        rows.is_empty() && total == 0,
        "with no adjudicated allowlist, nothing is publishable \u{2014} that is why retaining \
         the rows is safe"
    );

    // --- 2. de-admission spares the vetoed row --------------------------------
    //
    // The allowlist names a DIFFERENT peer rather than being empty. An empty one
    // would also de-admit `veto-peer.org`, but it is the one shape the sweep now
    // refuses outright -- a file naming nobody cannot be told apart from a truncated
    // write, see HOLE 7 -- so it would test the guard rather than de-admission.
    let revoked = store
        .revoke_peers_no_longer_admitted(&WorldsFederationPeers::Admitted {
            path: std::path::PathBuf::from("/etc/catalyrst/federation-peers.toml"),
            peers: vec![admit("still-admitted.org", "http://127.0.0.1:1/")],
            omitted: Vec::new(),
        })
        .await
        .expect("the sweep runs");
    assert_eq!(
        revoked.worlds_deleted(),
        1,
        "only the un-vetoed row is destroyed"
    );

    let remaining: Vec<String> =
        sqlx::query_scalar("SELECT world_name FROM remote_worlds ORDER BY world_name")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        remaining,
        vec!["vetoed.dcl.eth".to_string()],
        "the operator's veto is the one thing in this table that is ours, and it survives"
    );

    scratch.drop().await;
}

// HOLE 2 -- case-variant peer ids collapse into one mirror namespace  [CLOSED]
//
// Both tests below are INVERTED: they asserted the bug, they now assert the fix.
//
// What the bug was. `FederationRegistry::parse_file` keyed its map on the *raw*
// `peer_id` while `AdmittedPeer::admit` lowercased it. Two entries differing only in
// case were two distinct file entries -- two DAO proposals, two pinned roots, two
// hosts -- that minted ONE `PeerId`, and admission never noticed. Since
// `replace_peer_worlds` opens with `DELETE FROM remote_worlds WHERE peer_id = $1 AND
// hidden_since IS NULL`, whichever polled second silently erased the first's entire
// mirror, and which one that was got decided by an ASCII comparison on the raw id.
//
// What the fix is. `catalyrst_fed::canonical_peer_id` is now the single definition of
// a peer id's canonical form, both sides call it, and `parse_file` REFUSES a file
// whose entries collide under it, naming both spellings. Canonicalising is kept --
// host names are case-insensitive -- but a collision is an operator error reported at
// boot, not a merge performed in silence.

/// Render a peer-file entry to TOML, so these tests exercise the real
/// `FederationRegistry::parse_file` rather than a hand-built map.
fn peer_toml(c: &PeerCert) -> String {
    let key: Vec<String> = c.gossip_pubkey.iter().map(|b| b.to_string()).collect();
    format!(
        "[[peer]]\n\
         peer_id       = \"{}\"\n\
         catalyst_url  = \"{}\"\n\
         worlds_url    = \"{}\"\n\
         gossip_pubkey = [{}]\n\
         mtls_root_pem = \"{}\"\n\
         dao_proposal  = \"{}\"\n\
         added_at      = \"{}\"\n\n",
        c.peer_id,
        c.catalyst_url,
        c.worlds_url,
        key.join(","),
        c.mtls_root_pem,
        c.dao_proposal,
        c.added_at,
    )
}

fn write_peer_file(name: &str, entries: &[PeerCert]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("catalyrst-fed-audit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    let body: String = entries.iter().map(peer_toml).collect();
    std::fs::write(&path, body).expect("write peer file");
    path
}

/// **The fix, no DB required.** A peer file naming the same peer twice -- differing
/// only in case, which for a host name is not a difference at all (RFC 4343) -- is
/// refused at boot, and the refusal names *both* spellings so the operator can find
/// the two lines.
///
/// This is option (b) of the two the audit posed: ids are canonicalised once, at parse
/// time, and a collision is a refusal rather than a silent merge. Option (a) --
/// compare case-sensitively and treat the two as separate peers -- was rejected because
/// the `remote_worlds` CHECK constraints already assert `peer_id = lower(peer_id)`, so
/// two case-variant namespaces could not both be stored anyway; it would have moved
/// the collision from admission down into a database constraint violation on the
/// second poll.
///
/// Note what is deliberately *not* asserted: that `admit` distinguishes the two.
/// Canonicalising is correct -- `Peer.Example.ORG` and `peer.example.org` are one host.
/// The defect was never the fold; it was two components folding differently and
/// nobody counting the result.
#[test]
fn a_peer_file_naming_one_peer_twice_is_refused_at_boot_naming_both_entries() {
    let mut lower = cert("peer.example.org", "http://127.0.0.1:1/");
    lower.dao_proposal = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xAAA".into();
    let mut upper = cert("Peer.Example.ORG", "http://127.0.0.1:2/");
    upper.dao_proposal = "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xBBB".into();

    let path = write_peer_file("case-variant-twins.toml", &[lower, upper]);
    let err = WorldsFederationPeers::load_file(&path, &fed_config())
        .expect_err("a file naming one peer twice must not boot");
    let msg = format!("{err:#}");

    assert!(
        msg.contains("peer.example.org") && msg.contains("Peer.Example.ORG"),
        "the refusal must name BOTH entries as the operator wrote them, so the two \
         offending lines are findable; got: {msg}"
    );

    // And the canonical fold itself is unchanged and shared: one host, one id.
    assert_eq!(
        catalyrst_fed::canonical_peer_id("Peer.Example.ORG"),
        catalyrst_fed::canonical_peer_id("  peer.example.org "),
        "canonicalisation is still case- and whitespace-insensitive; what changed is \
         that a file relying on it to merge two entries is refused"
    );
}

/// The store-side consequence, inverted. Because a case-variant pair can no longer be
/// admitted at all, no two `AdmittedPeer`s in a process can share a `PeerId`, and
/// `replace_peer_worlds`'s opening `DELETE ... WHERE peer_id = $1` can only ever clear
/// rows belonging to the peer doing the writing.
///
/// This is the precondition Finding A's fix depends on: comparing mirror rows against
/// the admitted set is only meaningful once one id cannot stand for two entries.
#[tokio::test]
async fn distinct_peers_own_distinct_mirror_namespaces_and_no_id_stands_for_two_entries() {
    let Some(scratch) = setup_db().await else {
        skipped("distinct peers own distinct mirror namespaces");
        return;
    };
    let pool = scratch.pool.clone();
    let store = RemoteWorldsComponent::new(pool.clone());

    // The load that used to produce the collapse. Whatever this returns, it must not
    // be a peer set in which one `PeerId` stands for two entries -- that is the state
    // in which the second poll's `DELETE ... WHERE peer_id = $1` erases the first's
    // mirror. Refusing the file is how that is achieved; the assertion is written
    // against the *property*, not the mechanism, so it still holds if the mechanism
    // is ever replaced.
    let twins = write_peer_file(
        "case-variant-twins-store.toml",
        &[
            cert("peer.example.org", "http://127.0.0.1:1/"),
            cert("Peer.Example.ORG", "http://127.0.0.1:2/"),
        ],
    );
    if let Ok(loaded) = WorldsFederationPeers::load_file(&twins, &fed_config()) {
        let ids: Vec<_> = loaded.peers().iter().map(|p| p.peer_id().clone()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "two file entries minted one PeerId and admission did not notice: {ids:?}. \
             Both would poll, and whichever polled second would silently erase the \
             other's entire mirror."
        );
    }

    // A file with two genuinely distinct peers loads, and yields two distinct ids.
    let path = write_peer_file(
        "two-distinct-peers.toml",
        &[
            cert("alpha.example.org", "http://127.0.0.1:1/"),
            cert("beta.example.org", "http://127.0.0.1:2/"),
        ],
    );
    let loaded = WorldsFederationPeers::load_file(&path, &fed_config()).expect("two real peers");
    let ids: Vec<_> = loaded.peers().iter().map(|p| p.peer_id().clone()).collect();
    assert_eq!(ids.len(), 2, "both peers admitted");
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "no PeerId may stand for more than one admitted entry"
    );

    let a = loaded.get("alpha.example.org").expect("alpha admitted");
    let b = loaded.get("beta.example.org").expect("beta admitted");

    store
        .replace_peer_worlds(a.peer_id(), &[row(a.peer_id(), "from-alpha.dcl.eth")])
        .await
        .expect("alpha mirrors");
    store
        .replace_peer_worlds(b.peer_id(), &[row(b.peer_id(), "from-beta.dcl.eth")])
        .await
        .expect("beta mirrors");

    let (rows, total) = store
        .list_mirror(&loaded, None, 100, 0)
        .await
        .expect("list");
    assert_eq!(
        total, 2,
        "each peer keeps its own namespace; a poll must never wipe another peer's rows"
    );
    let mut names: Vec<_> = rows
        .iter()
        .map(|r| r.name.as_peer_reported_str().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["from-alpha.dcl.eth", "from-beta.dcl.eth"]);

    // Lookup is by canonical id, so an operator typing the peer in any case reaches
    // the same peer -- and cannot reach a *second* one, because there is no second one.
    assert_eq!(
        loaded
            .get("ALPHA.Example.Org")
            .expect("lookup folds case")
            .peer_id(),
        a.peer_id()
    );
}

// HOLE 3 -- the veto route answered an unauthenticated caller before authorizing

/// **CLOSED.** `set_mirror_world_hidden` used to take `Json<SetMirrorHiddenRequest>`,
/// and `Json` is an axum **extractor** -- extractors run before the handler body. So an
/// anonymous caller who sent a body that did not deserialise got 415 or 422 from the
/// extractor and never reached `authorize_admin`.
///
/// It was never a privilege escalation: nothing was written, and a *well-formed*
/// anonymous request was already 403. What it cost was the property the module docs
/// claim -- that the route authenticates its caller as its first statement -- and it made
/// the route an unauthenticated oracle for its own request schema.
///
/// The handler now takes `body: Bytes`, which cannot fail, and deserialises after the
/// check. This test asserts the inverted property: **every** anonymous request to this
/// route is 403, whatever the body or Content-Type, and the schema is only consulted
/// once the caller has proven who they are.
#[tokio::test]
async fn the_veto_route_authorizes_before_any_extractor_can_answer() {
    let Some(scratch) = setup_db().await else {
        skipped("the veto route authorizes before any extractor can answer");
        return;
    };
    let app = build_app(
        scratch.pool.clone(),
        WorldsFederationPeers::Admitted {
            path: std::path::PathBuf::from("/etc/catalyrst/federation-peers.toml"),
            peers: vec![admit("peer.example.org", "http://127.0.0.1:1/")],
            omitted: Vec::new(),
        },
    );

    let uri = "/admin/federation/worlds/peer.example.org/x.dcl.eth/hidden";

    // Each of these used to be answered by something other than authz. The bodies
    // are chosen to trip a different stage of the old extractor: no Content-Type at
    // all (was 415), right type and wrong schema (was 422), right type and
    // unparseable bytes (was 400), and an empty body (was 400).
    for (label, content_type, body) in [
        ("no content-type, not json", None, "not json"),
        (
            "well-formed json, wrong schema",
            Some("application/json"),
            r#"{"hidden":"yes"}"#,
        ),
        (
            "declared json, unparseable",
            Some("application/json"),
            "{{{",
        ),
        ("empty body", Some("application/json"), ""),
        (
            "well-formed and valid \u{2014} the one that was already 403",
            Some("application/json"),
            r#"{"hidden":true}"#,
        ),
    ] {
        let mut req = Request::builder().method("PUT").uri(uri);
        if let Some(ct) = content_type {
            req = req.header(header::CONTENT_TYPE, ct);
        }
        let (status, _) = call(&app, req.body(Body::from(body)).unwrap()).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "an anonymous caller must be refused by authz, not answered by an \
             extractor \u{2014} case: {label}"
        );
    }

    // And the schema IS still enforced, for a caller who has authenticated. The
    // distinction being asserted is who gets to learn about it, not whether it runs.
    let (status, body) = call(
        &app,
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"hidden":"yes"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an authenticated admin sending a body the schema rejects still gets an error: {body}"
    );

    scratch.drop().await;
}

// HOLE 4 -- the provenance grep gate had a blind spot in the files that matter

/// **CLOSED, by shape rather than by syntax.** The gate used to enforce two rules:
/// `as_peer_reported_str` may appear only in `fed/{names,wire,store,handlers}.rs`, and no
/// single line may contain both it and `from_request_path`.
///
/// The second was a *one-line* rule, and the laundering fits in two:
///
/// ```ignore
/// let raw = remote.name.as_peer_reported_str();
/// let local = LocalWorldName::from_request_path(raw);
/// ```
///
/// Inside `fed/handlers.rs` or `fed/store.rs` -- allowed to hold the escape hatch,
/// because they are exactly the files that hold a `RemoteWorldName` -- that passed both
/// checks. Making the line rule cleverer would only have moved the boundary; a third
/// line defeats a two-line rule.
///
/// The gate now bans the *local constructor* everywhere under `src/fed/` except
/// `fed/names.rs`, which defines it. That is a structural claim, not a pattern match:
/// nothing in `fed/` reads an HTTP request path, so nothing in `fed/` has any business
/// calling the constructor that interprets one. Together with the existing allowlist it
/// yields the property this test asserts -- **the two escape hatches are nameable in the
/// same file only in `fed/names.rs`** -- so no launder of any length has a file to live
/// in.
#[test]
fn the_two_escape_hatches_can_only_meet_in_the_file_that_defines_them() {
    let escape_hatch = concat!("as_peer_reported", "_str");
    let local_ctor = concat!("from_request", "_path");

    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).expect("src/ is readable") {
            let p = e.expect("dir entry").path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    walk(&src, &mut files);
    assert!(
        files.len() > 10,
        "the source walk found nothing; test is inert"
    );

    let mut both = Vec::new();
    for path in &files {
        let body = std::fs::read_to_string(path).expect("source file is readable");
        let code: Vec<&str> = body
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !(t.starts_with("//") || t.starts_with('*') || t.starts_with("#!"))
            })
            .collect();
        let has_hatch = code.iter().any(|l| l.contains(escape_hatch));
        let has_ctor = code.iter().any(|l| l.contains(local_ctor));
        if has_hatch && has_ctor {
            both.push(
                path.to_string_lossy()
                    .replace('\\', "/")
                    .rsplit("/src/")
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }

    assert_eq!(
        both,
        vec!["fed/names.rs".to_string()],
        "a peer-reported name and the local-name constructor may only be namable in the \
         same file where both are DEFINED. Any other file here can launder one into the \
         other across as many lines as it likes, and the type barrier stops meaning \
         anything."
    );

    // The shipped gate must actually carry the rule; otherwise the property above is a
    // coincidence of today's source that nothing defends tomorrow.
    let wire = std::fs::read_to_string(src.join("fed/wire.rs")).expect("fed/wire.rs is readable");
    assert!(
        wire.contains("mints a local world name inside the federation module"),
        "the gate no longer bans the local constructor inside fed/; the two-line launder \
         is reachable again"
    );
}

// HOLE 5 -- `insecureLoopback` reported `false` for a cleartext HTTP peer

/// **CLOSED, no DB required.** `AdmittedPeer::admit` used to set
///
/// ```ignore
/// insecure_loopback: loopback_opt_out && pem.is_empty(),
/// ```
///
/// so the flag meant "admitted over http *and* unpinned", not "admitted over http". A
/// peer configured as `http://127.0.0.1:PORT` **with** a valid `mtls_root_pem` under
/// `WORLDS_FED_ALLOW_INSECURE_LOOPBACK_PEERS=1` took the `tls_certs_only` branch, built
/// a pinned client, and was then talked to in cleartext, where a pin is exercised by
/// nothing. That combination reported `insecureLoopback: false` on
/// `GET /federation/worlds/peers` and `pinned = true` in the boot log -- precisely
/// backwards from the field's stated purpose, which is that a two-node test rig is never
/// mistaken for a federation deployment.
///
/// Both halves are fixed, and the second makes the first unreachable:
///
///  1. the flag is now `loopback_opt_out` alone, so it answers the question it is named
///     for -- is this channel authenticated -- and cleartext is the whole answer;
///  2. a non-empty `mtls_root_pem` on a cleartext URL is **refused at admission**
///     (`PeerNotAdmitted::PinnedRootOnCleartextUrl`), because a root pinned to a
///     connection with no TLS is an orphaned config field: read, stored, and inert. It
///     is the same defect the module refuses everywhere else, in a field that had been
///     spelled correctly.
#[test]
fn a_cleartext_http_peer_carrying_a_pinned_root_is_refused_as_a_contradiction() {
    // A real, self-signed root, so this is a valid pem being refused for where it is,
    // not a malformed one being refused for what it is.
    let mut params = rcgen::CertificateParams::new(Vec::new()).unwrap();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "audit-ca");
    let key = rcgen::KeyPair::generate().unwrap();
    let pem = params.self_signed(&key).unwrap().pem();

    let mut c = cert("cleartext-peer.org", "http://127.0.0.1:5242");
    c.mtls_root_pem = pem.clone();

    let err = match AdmittedPeer::admit(&c, &fed_config()) {
        Err(e) => e,
        other => panic!(
            "a pinned root over cleartext must be refused, not admitted as pinned: {other:?}"
        ),
    };
    assert!(
        matches!(
            err,
            catalyrst_worlds::fed::peers::PeerNotAdmitted::PinnedRootOnCleartextUrl { .. }
        ),
        "refused for the right reason, so the message tells the operator what to change: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("authenticates nothing"),
        "the message must say the pin is inert, not merely that the entry is invalid: {msg}"
    );

    // The same entry over https is admitted and pinned -- the refusal is about the
    // combination, not about either field.
    let mut https = cert("cleartext-peer.org", "https://peer.example.org");
    https.mtls_root_pem = pem;
    let peer = match AdmittedPeer::admit(&https, &fed_config()) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("the https form must be admitted: {other:?}"),
    };
    assert!(
        !peer.is_insecure_loopback(),
        "a pinned https peer is the secure case, and says so"
    );

    // And the dev escape hatch still works in its intended shape: cleartext loopback
    // with NO pem, reported honestly as insecure.
    let bare = cert("cleartext-peer.org", "http://127.0.0.1:5242");
    let peer = match AdmittedPeer::admit(&bare, &fed_config()) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("the loopback dev opt-out must still admit: {other:?}"),
    };
    assert_eq!(peer.worlds_url().scheme(), "http");
    assert!(
        peer.is_insecure_loopback(),
        "the one field an operator greps for unauthenticated channels must say so"
    );
}

// HOLE 6 -- a failed collision probe rendered as "no collisions"  [CLOSED]
//
// The test below is NEW rather than inverted: the audit demonstrated this finding
// against a live binary and left no test behind. It asserts the fix.
//
// What the bug was. `fed/poll.rs` matched on `local_names_also_claimed(..)` and
// returned `Vec::new()` from the `Err` arm, and `refresh_federation_mirror` rendered
// that as `localNameCollisions: []` -- byte-identical to the answer a server with no
// collisions gives. A database error was published as a measurement, in the one field
// whose job is to tell an operator that two servers are claiming one name. The failed
// arm of `poll_all` had the same shape for a second reason: a peer that was never
// reached was reported with `localNameCollisions: []`, an empty reading of a probe that
// never ran.
//
// What the fix is. `PollReport::collisions` is a `LocalNameCollisions`, which is either
// `Checked(names)` or `Unavailable(reason)` and has no accessor that flattens the two.
// The distinction is carried through all three layers:
//   - the poll outcome -- `Checked(vec![])` vs `Unavailable(e)`, asserted in
//     `tests/federation_mirror_poll.rs`;
//   - what is stored -- a successful poll with an unavailable probe records
//     `COLLISION_PROBE_UNAVAILABLE_PREFIX` in `remote_peer_status.last_error` *beside*
//     a fresh `last_success_at`, so the row says "current rows, one unchecked thing
//     about them";
//   - the JSON -- `localNameCollisions` is `null`, never `[]`, with a reason in
//     `localNameCollisionsError`.
//
// The probe-error path is covered in `tests/federation_mirror_routes.rs`, which has a
// stub peer to produce a *successful* poll. The test here covers the other arm, which
// needs no peer at all: a peer that cannot be reached.

/// **The fix.** A peer whose poll failed reports no collision list, rather than an
/// empty one.
///
/// The peer here is admitted and points at a port nothing is listening on, so the poll
/// fails in transport -- the commonest real failure, and the one where an operator is
/// most likely to be scanning the refresh output for what changed.
#[tokio::test]
async fn a_refresh_of_an_unreachable_peer_reports_no_collision_list_rather_than_an_empty_one() {
    let Some(scratch) = setup_db().await else {
        skipped(
            "a peer whose poll failed reports localNameCollisions: null with a reason, \
             never an empty list",
        );
        return;
    };

    // Port 1 on loopback: privileged, unbound, and refused immediately.
    let peer = admit("unreachable.dclone.org", "http://127.0.0.1:1");
    let app = build_app(
        scratch.pool.clone(),
        WorldsFederationPeers::Admitted {
            path: std::path::PathBuf::from("/nonexistent/peers.toml"),
            peers: vec![peer],
            omitted: Vec::new(),
        },
    );

    let (status, body) = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/admin/federation/worlds/refresh")
            .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let result = &body["polled"][0];
    assert_eq!(result["ok"], serde_json::json!(false));
    assert!(
        result["error"].as_str().is_some_and(|e| !e.is_empty()),
        "a failed poll says why: {result}"
    );
    // The zeroes are honest because `ok: false` is right next to them. An empty
    // collision list would not be: nothing was probed, so there is nothing to be empty.
    assert_eq!(
        result["localNameCollisions"],
        serde_json::json!(null),
        "PRE-FIX this was `[]`, indistinguishable from a clean probe: {result}"
    );
    assert_eq!(
        result["localNameCollisionsError"],
        serde_json::json!(catalyrst_worlds::fed::handlers::NOT_PROBED_POLL_FAILED),
        "and the null carries the reason it is null: {result}"
    );

    scratch.drop().await;
}

// HOLE 7 -- a peer file that names nobody was accepted, and destroyed the mirror
//
// Introduced BY the fix for HOLE 1, and only visible because of it.
//
// Every malformation of `federation-peers.toml` is a boot refusal -- a `TODO:`
// proposal, an epoch `added_at`, a zero gossip key, a reserved host suffix, a blank
// pinned root, unparseable TOML, a missing file, two entries naming one peer. There is
// exactly one way to write a file the loader accepts without complaint and that names
// no peers: leave it empty, truncate it, or misspell the table header. `PeerFile.peer`
// is `#[serde(default)]`, so `[[peers]]` -- one stray character -- parses to a document
// with zero entries and boots.
//
// Before the boot sweep existed that was harmless: federation did not happen
// and the mirrored rows sat there unpublished. The sweep changed what it costs.
// `is_configured()` is true for `Admitted { peers: [] }`, so the sweep takes its
// destructive branch, `peer_id <> ALL('{}')` is TRUE for every row, and one stray
// character in a config file silently deletes every mirrored row this server holds --
// on a boot that reports success and exits zero.
//
// This is not the documented "a file that admitted nobody is a different statement"
// case. That statement is one an operator makes by deleting entries. This one is made
// by a typo, and the two are the same observable state -- which is the exact confusion
// `WorldsFederationPeers`' two variants exist to prevent, reappearing one level up: the
// enum distinguishes "no file" from "a file naming nobody", and nothing distinguishes
// "a file naming nobody on purpose" from "a file naming nobody by accident".
//
// Demonstrated against the live binary, not only here. With three mirrored rows in the
// database and `WORLDS_FED_PEERS_FILE` pointing at a two-line file whose only defect is
// `[[peers]]` for `[[peer]]`:
//
//   INFO worlds federation peer registry loaded  admitted=0 omitted=0
//   WARN federation peer is no longer in the allowlist; its mirrored worlds have been
//        deleted ...  peer_id=peer-a.dclone.org worlds_deleted=2
//   WARN ... peer_id=peer-b.dclone.org worlds_deleted=1
//   INFO worlds mirror reconciled against the admitted set before serving
//        admitted=0 peers_revoked=2 worlds_deleted=3
//
// ...and the process went on to serve. `rows left: 0`.
//
// CLOSED, at the sweep rather than at the loader.
//
// Refusing the file was the first fix tried, and it was wrong: it made "federation
// is on and we currently admit nobody" inexpressible, which is the exact
// distinction `WorldsFederationPeers`' two variants exist to carry. An operator who
// removes the last entry from the file is making a real statement, and the loader
// is not the place that can tell that statement apart from a truncated write.
//
// The damage was never the empty set. It was that an empty admitted set made the
// sweep's `peer_id <> ALL('{}')` true for every row. So the empty set is now
// refused *where it destroys data*: `revoke_peers_no_longer_admitted` returns
// `NoAllowlistToEnforce` without deleting anything, and logs a warning that names
// the `[[peer]]`/`[[peers]]` trap. The mirrored rows stay, published by nothing --
// which is the same end state a correct empty allowlist produces, minus the
// irreversible delete.
//
// The two tests below now assert that: the file loads, the sweep declines, the rows
// survive.

/// A peer file whose table header is misspelled loads as an allowlist naming nobody --
/// and the boot sweep refuses to act on it, so the mirror survives the typo.
#[tokio::test]
async fn a_peer_file_with_the_section_header_mistyped_does_not_sweep_the_mirror() {
    let Some(scratch) = setup_db().await else {
        skipped("a mistyped section header loads as empty, and the sweep declines to act on it");
        return;
    };
    let store = RemoteWorldsComponent::new(scratch.pool.clone());
    let peer = admit("peer-a.dclone.org", "http://127.0.0.1:1/");
    store
        .replace_peer_worlds(peer.peer_id(), &[row(peer.peer_id(), "kept.dcl.eth")])
        .await
        .expect("mirror the peer");

    let dir = std::env::temp_dir().join(format!("catalyrst-fed-typo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("mistyped.toml");
    // [[peers]] not [[peer]] - one character, and still valid TOML. PeerFile.peer
    // is #[serde(default)], so this parses to zero entries and loads.
    std::fs::write(
        &path,
        "[[peers]]\nversion = 1\npeer_id = \"peer-a.dclone.org\"\ncatalyst_url = \"https://peer-a.dclone.org/content\"\n",
    )
    .expect("write mistyped peer file");

    let peers = WorldsFederationPeers::load_file(&path, &fed_config())
        .expect("a zero-entry file still loads; the empty set is answered at the sweep");
    assert!(
        peers.is_configured() && peers.peers().is_empty(),
        "the typo is invisible here by construction - that is why the sweep has to be \
         the one to refuse"
    );

    // CLOSED: the sweep declines rather than deleting every row.
    let outcome = store
        .revoke_peers_no_longer_admitted(&peers)
        .await
        .expect("the sweep must not error");
    assert!(
        matches!(outcome, Revocation::NoAllowlistToEnforce),
        "an allowlist admitting nobody must not sweep, got {outcome:?}"
    );

    // Count the ROWS, not the published view. list_mirror filters against the
    // admitted set and so reports 0 here whether or not the sweep destroyed the
    // table. The property under test is that the destructive delete never ran.
    let survived: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_worlds")
        .fetch_one(&scratch.pool)
        .await
        .expect("count mirrored rows");
    assert_eq!(
        survived, 1,
        "declining the sweep must leave the mirrored rows in place"
    );

    scratch.drop().await;
}

/// The same shape with nothing to misread at all: a zero-byte file.
///
/// Kept separate because it is what a truncated write, a failed template render or an
/// empty ConfigMap key produces, and because it removes any argument that the typo case
/// is about serde tolerating unknown keys. There is no content here to tolerate.
#[tokio::test]
async fn a_zero_byte_peer_file_loads_as_an_empty_allowlist_that_does_not_sweep() {
    let Some(scratch) = setup_db().await else {
        skipped("a zero-byte peer file loads as an empty allowlist that does not sweep");
        return;
    };
    let store = RemoteWorldsComponent::new(scratch.pool.clone());
    let peer = admit("peer-a.dclone.org", "http://127.0.0.1:1/");
    store
        .replace_peer_worlds(peer.peer_id(), &[row(peer.peer_id(), "kept.dcl.eth")])
        .await
        .expect("mirror the peer");

    let dir = std::env::temp_dir().join(format!("catalyrst-fed-audit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("zero-bytes.toml");
    std::fs::write(&path, "").expect("write empty peer file");

    let peers = WorldsFederationPeers::load_file(&path, &fed_config())
        .expect("an empty file is a legitimate 'we admit nobody', and still loads");
    assert!(peers.is_configured() && peers.peers().is_empty());

    // CLOSED: the sweep refuses an empty admitted set, so `peer_id <> ALL('{}')`
    // never reaches the DELETE. Disabling federation outright is still done by
    // unsetting the path (NotConfigured); this is the softer, non-destructive state.
    let outcome = store
        .revoke_peers_no_longer_admitted(&peers)
        .await
        .expect("the sweep must not error");
    assert!(
        matches!(outcome, Revocation::NoAllowlistToEnforce),
        "an allowlist admitting nobody must not sweep, got {outcome:?}"
    );
    assert_eq!(outcome.worlds_deleted(), 0);

    let survived: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_worlds")
        .fetch_one(&scratch.pool)
        .await
        .expect("count mirrored rows");
    assert_eq!(
        survived, 1,
        "declining the sweep must leave the mirrored rows in place"
    );

    scratch.drop().await;
}

/// A smaller, adjacent inaccuracy in the same sweep, kept honest by its own test.
///
/// A peer that is **in** the file -- its DAO proposal intact, its root pinned -- but
/// which carries no `worlds_url` is `Omitted`, not `Admitted`. `admitted_ids` is built
/// from `peers()`, which excludes omissions, so the sweep deletes that peer's rows.
/// Deleting them is right: it is not a worlds peer, and nothing should publish its
/// rows. The WARN that fires is not right -- it says the peer "is no longer in the
/// allowlist", and it is. An operator reading a boot log after removing a `worlds_url`
/// is told the DAO dropped a peer it did not drop.
///
/// Verified against the live binary: with `worlds_url = ""` on an entry that is
/// otherwise unchanged and fully pinned, boot logs `federation peer omitted: ...` and
/// then, four lines later, `federation peer is no longer in the allowlist ...
/// peer_id=peer-a.dclone.org worlds_deleted=2` for that same peer.
///
/// **CLOSED.** The rows still go, and both the returned `RevokedPeer.because` and the
/// WARN it drives now say the peer stopped running a worlds server rather than that the
/// DAO dropped it.
#[tokio::test]
async fn a_peer_omitted_for_having_no_worlds_url_is_swept_as_an_omission_not_a_de_admission() {
    let Some(scratch) = setup_db().await else {
        skipped("an omitted peer's sweep is reported as an omission, not as a de-admission");
        return;
    };
    let store = RemoteWorldsComponent::new(scratch.pool.clone());

    let peer = admit("still-in-the-file.dclone.org", "http://127.0.0.1:1/");
    let peer_id = peer.peer_id().clone();
    store
        .replace_peer_worlds(&peer_id, &[row(&peer_id, "kept.dcl.eth")])
        .await
        .expect("mirror the peer");

    // Same entry, still adjudicated, not a worlds peer any more.
    let omitted = match AdmittedPeer::admit(&cert(peer_id.as_str(), ""), &{
        let mut c = fed_config();
        c.allow_insecure_loopback_peers = false;
        c
    }) {
        Ok(AdmissionOutcome::Omitted(o)) => o,
        // With no worlds_url and no pinned root the entry is refused outright, which is
        // its own documented behaviour; build the omission directly in that case.
        _ => catalyrst_worlds::fed::peers::PeerOmitted::NoWorldsUrl {
            peer_id: peer_id.as_str().to_string(),
        },
    };

    let swept = store
        .revoke_peers_no_longer_admitted(&WorldsFederationPeers::Admitted {
            path: std::path::PathBuf::from("/etc/catalyrst/federation-peers.toml"),
            peers: Vec::new(),
            omitted: vec![omitted],
        })
        .await
        .expect("the boot sweep runs");

    assert_eq!(
        swept.worlds_deleted(),
        1,
        "the rows go, which is correct \u{2014} an omitted peer is not a worlds peer"
    );
    assert_eq!(
        swept.revoked_peer_ids(),
        vec![peer_id.as_str()],
        "and it is reported, because something did stop being published"
    );

    // CLOSED: the reason travels in the value, not only in a log line a human might
    // read. `admitted_ids` is still built from `peers()` alone -- that is correct, an
    // omitted peer is not admitted -- so the distinction is drawn against `omitted()`.
    let Revocation::Swept { revoked, .. } = &swept else {
        panic!("a file that names a peer sweeps: {swept:?}");
    };
    assert_eq!(
        revoked[0].because,
        SweptBecause::StillListedButRunsNoWorldsServer,
        "this peer IS in the allowlist, with its DAO proposal intact; it simply runs no \
         worlds server. Reporting it as a de-admission sends an operator looking for a \
         governance decision that never happened."
    );

    scratch.drop().await;
}

// HOLE 8 -- the collision probe's database errors reached the public peers route
//
// Introduced BY the fix for HOLE 6, and narrowly.
//
// Before that fix, a failed collision probe wrote nothing to `remote_peer_status`: the
// `Err` arm returned `Vec::new()` and `record_success` had already cleared
// `last_error`. The fix restates the gap with `record_failure`, which is the right
// call -- a clean status row would put the fabrication back in the first place an
// operator looks -- but `record_failure` writes the error verbatim, and
// `remote_peer_status.last_error` is served, unauthenticated, as
// `GET /federation/worlds/peers` -> `peers[].status.lastError`.
//
// So a `sqlx::Error` from a query against our own `worlds` table is now published to
// anyone. Observed on the live binary, from an unauthenticated request:
//
//   "lastError": "mirror replaced; local name collision probe unavailable: error
//    returned from database: relation \"worlds\" does not exist at line 1469"
//
// A local table name and a source line number, on a public route, from an internal
// fault the caller had nothing to do with.
//
// CLOSED, by drawing the line between the FACT and the TEXT rather than by hiding
// the field. The fact that the probe did not run is what HOLE 6 put there and it
// stays public; the database's own words do not.
//
// `PollFailure::published()` is the general form: identical to `Display` for every
// variant whose text is about the *peer* -- their host, their HTTP status, their
// malformed body -- and a bounded constant for `Store`, whose text is about our
// database. The collision probe's restatement gets the same treatment via
// `PROBE_FAULT_PUBLIC_REASON`. Both verbatim strings are logged at the moment they
// are redacted, and the admin-only refresh route still returns the probe's own
// reason as `localNameCollisionsError`.

/// **Asserts the fix.** The unavailable probe is still reported on the public route,
/// and the raw database text is not.
///
/// Written against the store rather than a stub peer, because the store is where the
/// text is committed and the route is a pure read of it.
#[tokio::test]
async fn a_database_error_text_is_not_published_on_the_public_peers_route() {
    let Some(scratch) = setup_db().await else {
        skipped("internal database error text does not reach the public peers route");
        return;
    };
    let store = RemoteWorldsComponent::new(scratch.pool.clone());
    let peer = admit("leaky.dclone.org", "http://127.0.0.1:1/");

    // Exactly what `poll_peer` now does when the fetch and the write succeeded and the
    // collision probe did not: a success, then the gap restated with a bounded reason.
    store
        .record_success(peer.peer_id(), 2, 0, false)
        .await
        .expect("record the successful fetch");
    store
        .record_failure(
            peer.peer_id(),
            &format!(
                "{}{}",
                catalyrst_worlds::fed::poll::COLLISION_PROBE_UNAVAILABLE_PREFIX,
                catalyrst_worlds::fed::poll::PROBE_FAULT_PUBLIC_REASON
            ),
        )
        .await
        .expect("restate the unavailable probe");

    let app = build_app(
        scratch.pool.clone(),
        WorldsFederationPeers::Admitted {
            path: std::path::PathBuf::from("/nonexistent/peers.toml"),
            peers: vec![peer],
            omitted: Vec::new(),
        },
    );

    // No credential of any kind.
    let (status, body) = call(
        &app,
        Request::builder()
            .uri("/federation/worlds/peers")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the peers route is public by design"
    );

    let last_error = body["peers"][0]["status"]["lastError"]
        .as_str()
        .expect("the unavailable probe is reported, which is HOLE 6's fix working");

    // The fact is published, and should be.
    assert!(
        last_error.contains("collision probe unavailable"),
        "HOLE 6's fix must not regress: the unchecked probe stays visible: {last_error}"
    );
    // And nothing about our schema is.
    for leak in [
        "relation ",
        "at line ",
        "sqlx",
        "remote_worlds",
        "\"worlds\"",
    ] {
        assert!(
            !last_error.contains(leak),
            "the public reason must carry no internal detail; found {leak:?} in: {last_error}"
        );
    }

    // And it sits beside a fresh success, which is the pairing HOLE 6 established.
    assert!(
        body["peers"][0]["status"]["lastSuccessAt"].is_string(),
        "current rows AND an unchecked thing about them"
    );

    scratch.drop().await;
}

/// The redaction is a property of `PollFailure`, not of one call site, so it is asserted
/// there too: peer-derived text survives, ours does not.
#[test]
fn only_our_own_database_text_is_withheld_from_the_published_reason() {
    use catalyrst_worlds::fed::poll::{PollFailure, STORE_FAULT_PUBLIC_REASON};

    let ours = PollFailure::Store("relation \"worlds\" does not exist at line 1469".into());
    assert_eq!(ours.published(), STORE_FAULT_PUBLIC_REASON);
    assert!(
        ours.to_string().contains("at line 1469"),
        "Display stays verbatim \u{2014} the log line is the whole point of keeping it"
    );

    // Facts about the peer are facts about a public federation link, and publish.
    for theirs in [
        PollFailure::Status(503),
        PollFailure::NotJson("text/html".into()),
        PollFailure::BodyTooLarge(4096),
        PollFailure::Transport("connection refused".into()),
        PollFailure::Malformed("missing field `worlds`".into()),
    ] {
        assert_eq!(
            theirs.published(),
            theirs.to_string(),
            "a peer-derived reason must not be redacted: an operator on either side of \
             the link needs it to diagnose the link"
        );
    }
}
