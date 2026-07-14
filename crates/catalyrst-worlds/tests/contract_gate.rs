use std::sync::Arc;

use axum::Router;
use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{
    create_simple_auth_chain, multipart_body, test_wallet, Case, Gate, MultipartPart, Wallet,
};
use catalyrst_fed::PeerCert;
use catalyrst_worlds::config::Config;
use catalyrst_worlds::fed::config::WorldsFedConfig;
use catalyrst_worlds::fed::peers::{AdmissionOutcome, AdmittedPeer, WorldsFederationPeers};
use catalyrst_worlds::fed::poll::WorldsMirror;
use catalyrst_worlds::livekit::world_room_name;
use catalyrst_worlds::ports::bans::BansComponent;
use catalyrst_worlds::ports::denylist::DenyListComponent;
use catalyrst_worlds::ports::name_denylist::NameDenyListChecker;
use catalyrst_worlds::ports::presence::PeersRegistry;
use catalyrst_worlds::ports::worlds::WorldsComponent;
use catalyrst_worlds::rate_limiter::RateLimiter;
use catalyrst_worlds::{api_router_with_spec, AppState, AppStateInner};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const ADMIN_TOKEN: &str = "cg-worlds-admin";
const WORLD: &str = "gate.dcl.eth";
const LIVEKIT_KEY: &str = "devkey";
const LIVEKIT_SECRET: &str = "devsecret";

// `ens.owner_id` deliberately holds the registrar caller rather than `owner`:
// that is what the squid records, so ownership must resolve through `nft`.
async fn squid_fixture(pool: &PgPool, owner: &str) {
    sqlx::query("CREATE SCHEMA squid_marketplace")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE squid_marketplace.ens (id text PRIMARY KEY, subdomain text NOT NULL, owner_id text NOT NULL)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE squid_marketplace.nft (id text PRIMARY KEY, ens_id text, category text NOT NULL, owner_id text NOT NULL)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO squid_marketplace.ens (id, subdomain, owner_id) VALUES ($1, $2, $3)")
        .bind("ens-gate")
        .bind("gate")
        .bind("0xbe92b49aee993adea3a002adcda189a2b7dec56c-ETHEREUM")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO squid_marketplace.nft (id, ens_id, category, owner_id) VALUES ($1, $2, 'ens', $3)",
    )
    .bind("nft-ens-gate")
    .bind("ens-gate")
    .bind(format!("{}-ETHEREUM", owner))
    .execute(pool)
    .await
    .unwrap();
}

fn test_config(contents_dir: std::path::PathBuf, federation: WorldsFedConfig) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 5146,
        database_url: "unused".into(),
        http_base_url: "http://gate.test".into(),
        network_id: 1,
        squid_database_url: None,
        global_scenes_urn: None,
        content_public_url: "http://gate.test/content".into(),
        lambdas_public_url: "http://gate.test/lambdas".into(),
        livekit_host: "livekit.gate.test".into(),
        livekit_ws_url: "wss://livekit.gate.test".into(),
        livekit_api_key: LIVEKIT_KEY.into(),
        livekit_api_secret: LIVEKIT_SECRET.into(),
        livekit_configured: true,
        livekit_webhook_key: None,
        max_users_per_world: 100,
        comms_offline_when_unreachable: true,
        realm_name_strip_ens: true,
        preview_wearable_urns: Vec::new(),
        contents_upstream_url: Some("http://127.0.0.1:9".into()),
        contents_dir,
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

fn build_state(
    pool: PgPool,
    contents_dir: std::path::PathBuf,
    federation: WorldsFedConfig,
    peers: WorldsFederationPeers,
) -> AppState {
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    Arc::new(AppStateInner {
        sfu: catalyrst_livekit::SfuHealth::always_alive(),
        cfg: test_config(contents_dir, federation.clone()),
        worlds: WorldsComponent::new(pool.clone()),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(),
        bans: BansComponent::new(http.clone(), None, None),
        denylist: DenyListComponent::new(http.clone(), None),
        name_denylist: NameDenyListChecker::new(http.clone(), None),
        http,
        squid_pool: Some(pool.clone()),
        mirror: WorldsMirror::new(pool, federation, &peers),
        fed_peers: peers,
    })
}

fn deploy_multipart(wallet: &Wallet, thumb: &[u8]) -> (Vec<u8>, String, String, String) {
    deploy_multipart_with_thumbnail(wallet, thumb, "thumb.png")
}

fn deploy_multipart_with_thumbnail(
    wallet: &Wallet,
    thumb: &[u8],
    nav_thumb: &str,
) -> (Vec<u8>, String, String, String) {
    let thumb_hash = catalyrst_hashing::hash_bytes_v1(thumb);
    let entity = json!({
        "type": "scene",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "pointers": ["0,0", "0,1"],
        "content": [{ "file": nav_thumb, "hash": thumb_hash }],
        "metadata": {
            "display": { "title": "Gate World", "navmapThumbnail": nav_thumb },
            "worldConfiguration": { "name": WORLD },
            "scene": { "base": "0,0", "parcels": ["0,0", "0,1"] }
        }
    });
    let entity_bytes = serde_json::to_vec(&entity).unwrap();
    let entity_id = catalyrst_hashing::hash_bytes_v1(&entity_bytes);
    let chain = create_simple_auth_chain(wallet, &entity_id).unwrap();
    let (body, content_type) = multipart_body(&[
        MultipartPart::field("entityId", &entity_id),
        MultipartPart::field("authChain", &chain.to_string()),
        MultipartPart::file(
            "entity.json",
            "entity.json",
            "application/json",
            entity_bytes,
        ),
        MultipartPart::file("thumb.png", "thumb.png", "image/png", thumb.to_vec()),
    ]);
    (body, content_type, entity_id, thumb_hash)
}

/// Mints what LiveKit actually sends: an HS256 JWT over the body digest, bounded by
/// `exp`/`nbf`.
///
/// The lifetime claims are not decoration. `catalyrst_livekit::verify_webhook_token`
/// requires `exp` and refuses a token without one, because an unexpirable webhook
/// credential is replayable forever -- so a fixture that omits `exp` is not a valid
/// webhook, and a 401 for it is the correct answer. Sign with `secret` so a caller
/// can also mint the wrong-key token the 401 case needs.
fn webhook_jwt(body: &[u8], secret: &str) -> String {
    let now = chrono::Utc::now().timestamp() as u64;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims = json!({
        "iss": LIVEKIT_KEY,
        "nbf": now,
        "exp": now + 300,
        "sha256": B64_STANDARD.encode(Sha256::digest(body)),
    });
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    let signing_input = format!("{}.{}", header, payload);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{}.{}", signing_input, sig)
}

// The federated half of the surface.
//
// With federation off -- the state the rest of this file runs in, and the state most
// deployments run in -- all four /federation/worlds/* routes answer 503 before they
// read anything. A gate that only ever saw the 503 would leave every 200 body (the
// peer list, the mirror listing, the refresh report, the veto ack) unvalidated
// against its spec schema, which is coverage in the tally and nothing on the wire.
// So the suite raises a second app, federated against a stub peer on loopback, and
// drives both halves into the same Gate.
//
// The stub peer is only ever a source of *content* claims. It is never consulted for
// authority, and nothing below reads an owner out of its payload.

const FED_PEER_ID: &str = "gate-peer.dclone.org";
const FED_PEER_WORLD: &str = "mirrored.dcl.eth";

fn fed_config() -> WorldsFedConfig {
    WorldsFedConfig {
        // Never opened: the peer set below is constructed directly, so admission is
        // exercised without making this suite depend on a file on disk.
        peers_file: Some(std::path::PathBuf::from("/nonexistent/gate-peers.toml")),
        poll_interval_secs: 300,
        max_response_bytes: 4 * 1024 * 1024,
        max_worlds_per_peer: 10_000,
        allow_insecure_loopback_peers: true,
    }
}

/// A peer publishing one world, with the ownership claim a real peer sends on every
/// entry -- present precisely so the mirror's refusal to propagate it is exercised by
/// the schema check rather than assumed.
async fn start_stub_peer() -> String {
    let body = json!({
        "worlds": [{
            "name": FED_PEER_WORLD,
            "title": "Mirrored World",
            "owner": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "deployed_scenes": 2
        }],
        "total": 1
    })
    .to_string();
    let app = Router::new().route(
        "/worlds",
        axum::routing::get(move || {
            let body = body.clone();
            async move { ([("content-type", "application/json")], body) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://127.0.0.1:{}", addr.port())
}

fn admit_stub_peer(worlds_url: &str, cfg: &WorldsFedConfig) -> AdmittedPeer {
    let cert = PeerCert {
        version: 1,
        peer_id: FED_PEER_ID.to_string(),
        catalyst_url: "https://peer.dclone.org".to_string(),
        gossip_pubkey: [7u8; 32],
        mtls_root_pem: String::new(),
        dao_proposal: "https://snapshot.org/#/snapshot.dcl.eth/proposal/0xabc123".to_string(),
        added_at: "2026-07-01".to_string(),
        worlds_url: worlds_url.to_string(),
    };
    match AdmittedPeer::admit(&cert, cfg) {
        Ok(AdmissionOutcome::Admitted(p)) => p,
        other => panic!("gate fixture peer must be admitted, got {other:?}"),
    }
}

#[tokio::test]
async fn every_spec_route_answers_its_contract() {
    let Some(scratch) = ScratchDb::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds").await else {
        return;
    };
    scratch
        .apply_sql(include_str!("../migrations/0001_init.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0002_access_log.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0003_permission_parcels.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0004_lower_name_indexes.sql"))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0005_federation_remote_worlds.sql"
        ))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0006_federation_deadmission.sql"
        ))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0010_world_settings_version.sql"
        ))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0011_world_scenes_updated_at.sql"
        ))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0012_world_realm_name_override.sql"
        ))
        .await;
    scratch
        .apply_sql(include_str!(
            "../migrations/0013_world_preview_wearables.sql"
        ))
        .await;

    let owner = test_wallet(7);
    let stranger = test_wallet(9);
    let outsider = test_wallet(13);
    squid_fixture(&scratch.pool, &owner.address().to_lowercase()).await;

    let contents_dir = std::env::temp_dir().join(format!("cg-worlds-{}", scratch.database));
    std::fs::create_dir_all(&contents_dir).unwrap();
    let (router, spec) = api_router_with_spec();
    let state = build_state(
        scratch.pool.clone(),
        contents_dir.clone(),
        // Federation off. These suites assert the *unfederated* behaviour of every
        // route they touch, and that behaviour must be identical with the mirror
        // compiled in -- which is exactly what leaving this at the default proves.
        WorldsFedConfig::default(),
        WorldsFederationPeers::NotConfigured,
    );
    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());
    for (m, p) in [
        ("post", "/gc"),
        ("post", "/get-comms-adapter/{room_id}"),
        ("get", "/ipfs/{hash}"),
        ("head", "/ipfs/{hash}"),
    ] {
        gate.waive_success(m, p, "added 2026-08-19; contract coverage owed");
        gate.waive_error(m, p, "added 2026-08-19; contract coverage owed");
    }

    // The same spec, served by a second app that *is* federated. Both routers come
    // from `api_router_with_spec`, so every case below is checked against the one
    // spec this gate was built from.
    let stub_peer_url = start_stub_peer().await;
    let fed_cfg = fed_config();
    let fed_peers = WorldsFederationPeers::Admitted {
        path: std::path::PathBuf::from("/nonexistent/gate-peers.toml"),
        peers: vec![admit_stub_peer(&stub_peer_url, &fed_cfg)],
        omitted: Vec::new(),
    };
    let fed_app: Router = api_router_with_spec().0.with_state(build_state(
        scratch.pool.clone(),
        contents_dir.clone(),
        fed_cfg,
        fed_peers,
    ));

    gate.hit(&app, Case::new("get", "/status")).await;
    gate.hit(&app, Case::new("get", "/live-data")).await;
    gate.hit(&app, Case::new("post", "/gc").expect(403)).await;
    gate.hit(
        &app,
        Case::new("post", "/get-comms-adapter/{room_id}")
            .path("/get-comms-adapter/badroom")
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/ipfs/{hash}")
            .path("/ipfs/notacid")
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("head", "/ipfs/{hash}")
            .path("/ipfs/notacid")
            .expect(400),
    )
    .await;

    let thumb = vec![0u8, 1, 2, 3, 4, 5, 6, 7];
    let (body, content_type, scene_id, thumb_hash) = deploy_multipart(&owner, &thumb);
    gate.hit(
        &app,
        Case::new("post", "/entities").body(body, &content_type),
    )
    .await;
    let (junk, junk_type) = multipart_body(&[MultipartPart::field("something", "else")]);
    gate.hit(
        &app,
        Case::new("post", "/entities")
            .body(junk, &junk_type)
            .expect(400),
    )
    .await;
    let (bad_thumb_body, bad_thumb_type, _, _) =
        deploy_multipart_with_thumbnail(&owner, &thumb, "https://example.com/image.png");
    gate.hit(
        &app,
        Case::new("post", "/entities")
            .body(bad_thumb_body, &bad_thumb_type)
            .expect(400),
    )
    .await;

    gate.hit(&app, Case::new("get", "/worlds")).await;
    gate.hit(&app, Case::new("get", "/index")).await;

    gate.hit(
        &app,
        Case::new("post", "/entities/active").json(&json!({ "pointers": ["0,0"] })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/entities/active")
            .json(&json!({}))
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/about").path(&format!("/world/{}/about", WORLD)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/about")
            .path("/world/nope.dcl.eth/about")
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/manifest")
            .path(&format!("/world/{}/manifest", WORLD)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/manifest")
            .path("/world/nope.dcl.eth/manifest")
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/scenes").path(&format!("/world/{}/scenes", WORLD)),
    )
    .await;
    gate.waive_error(
        "get",
        "/world/{world_name}/scenes",
        "unknown worlds answer 200 with an empty scene list; the documented 404 is unreachable",
    );

    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/preview-wearables")
            .path(&format!("/world/{}/preview-wearables", WORLD)),
    )
    .await;
    gate.waive_error(
        "get",
        "/world/{world_name}/preview-wearables",
        "a world previewing nothing answers 200 with an empty list, and an unreachable content \
         server degrades to the same shape rather than erroring mid-entry; the documented 500 is \
         reachable only from a worlds-database failure",
    );

    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/settings")
            .path(&format!("/world/{}/settings", WORLD)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/settings")
            .path("/world/nope.dcl.eth/settings")
            .expect(404),
    )
    .await;

    let settings_path = format!("/world/{}/settings", WORLD);
    let (sbody, stype) = multipart_body(&[MultipartPart::field("title", "Gate World Renamed")]);
    gate.hit(
        &app,
        Case::new("put", "/world/{world_name}/settings")
            .path(&settings_path)
            .signed(&owner)
            .body(sbody, &stype),
    )
    .await;
    let (sbody, stype) = multipart_body(&[MultipartPart::field("title", "Gate World Renamed")]);
    gate.hit(
        &app,
        Case::new("put", "/world/{world_name}/settings")
            .path(&settings_path)
            .body(sbody, &stype)
            .expect(400),
    )
    .await;
    let (big_body, big_type) = multipart_body(&[MultipartPart::file(
        "thumbnail",
        "thumbnail.png",
        "image/png",
        vec![0u8; 2 * 1024 * 1024 + 1],
    )]);
    gate.hit(
        &app,
        Case::new("put", "/world/{world_name}/settings")
            .path(&settings_path)
            .signed(&owner)
            .body(big_body, &big_type)
            .expect(400),
    )
    .await;

    let perms_path = format!("/world/{}/permissions", WORLD);
    gate.hit(
        &app,
        Case::new("get", "/world/{world_name}/permissions").path(&perms_path),
    )
    .await;
    gate.waive_error(
        "get",
        "/world/{world_name}/permissions",
        "unknown worlds answer 200 with default permissions; the documented 404 is unreachable",
    );

    let deploy_perm_path = format!("/world/{}/permissions/deployment", WORLD);
    gate.hit(
        &app,
        Case::new("post", "/world/{world_name}/permissions/{permission_name}")
            .path(&deploy_perm_path)
            .signed_meta(
                &owner,
                &json!({ "type": "allow-list", "wallets": [stranger.address()] }),
            )
            .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/world/{world_name}/permissions/{permission_name}")
            .path(&format!("/world/{}/permissions/bogus", WORLD))
            .signed_meta(&owner, &json!({ "type": "allow-list" }))
            .expect(400),
    )
    .await;

    let addr_path = format!(
        "/world/{}/permissions/streaming/{}",
        WORLD,
        stranger.address()
    );
    gate.hit(
        &app,
        Case::new(
            "put",
            "/world/{world_name}/permissions/{permission_name}/{address}",
        )
        .path(&addr_path)
        .signed(&owner)
        .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "put",
            "/world/{world_name}/permissions/{permission_name}/{address}",
        )
        .path(&format!("/world/{}/permissions/streaming/zzz", WORLD))
        .signed(&owner)
        .expect(400),
    )
    .await;

    let parcels_path = format!(
        "/world/{}/permissions/deployment/address/{}/parcels",
        WORLD,
        stranger.address()
    );
    gate.hit(
        &app,
        Case::new(
            "post",
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
        )
        .path(&parcels_path)
        .signed(&owner)
        .json(&json!({ "parcels": ["0,0"] }))
        .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "post",
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
        )
        .path(&format!(
            "/world/{}/permissions/bogus/address/{}/parcels",
            WORLD,
            stranger.address()
        ))
        .signed(&owner)
        .json(&json!({ "parcels": [] }))
        .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new(
            "get",
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
        )
        .path(&parcels_path),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "get",
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
        )
        .path(&format!(
            "/world/{}/permissions/streaming/address/{}/parcels",
            WORLD,
            owner.address()
        ))
        .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new(
            "post",
            "/world/{world_name}/permissions/{permission_name}/parcels",
        )
        .path(&format!("/world/{}/permissions/deployment/parcels", WORLD))
        .json(&json!({ "parcels": ["0,0"] })),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "post",
            "/world/{world_name}/permissions/{permission_name}/parcels",
        )
        .path(&format!("/world/{}/permissions/bogus/parcels", WORLD))
        .json(&json!({ "parcels": ["0,0"] }))
        .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new(
            "delete",
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
        )
        .path(&parcels_path)
        .signed(&owner)
        .json(&json!({ "parcels": ["0,0"] }))
        .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "delete",
            "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
        )
        .path(&format!(
            "/world/{}/permissions/bogus/address/{}/parcels",
            WORLD,
            stranger.address()
        ))
        .signed(&owner)
        .json(&json!({ "parcels": [] }))
        .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new(
            "delete",
            "/world/{world_name}/permissions/{permission_name}/{address}",
        )
        .path(&addr_path)
        .signed(&owner)
        .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "delete",
            "/world/{world_name}/permissions/{permission_name}/{address}",
        )
        .path(&format!("/world/{}/permissions/streaming/zzz", WORLD))
        .signed(&owner)
        .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("post", "/world/{world_name}/permissions/{permission_name}")
            .path(&format!("/world/{}/permissions/access", WORLD))
            .signed_meta(
                &owner,
                &json!({ "type": "allow-list", "wallets": [], "communities": [] }),
            )
            .expect(204),
    )
    .await;

    let community_path = format!(
        "/world/{}/permissions/access/communities/gate-community-1",
        WORLD
    );
    gate.hit(
        &app,
        Case::new(
            "put",
            "/world/{world_name}/permissions/access/communities/{communityId}",
        )
        .path(&community_path)
        .signed(&owner)
        .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "put",
            "/world/{world_name}/permissions/access/communities/{communityId}",
        )
        .path(&format!(
            "/world/{}/permissions/access/communities/%20",
            WORLD
        ))
        .signed(&owner)
        .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "delete",
            "/world/{world_name}/permissions/access/communities/{communityId}",
        )
        .path(&community_path)
        .signed(&owner)
        .expect(204),
    )
    .await;
    gate.hit(
        &app,
        Case::new(
            "delete",
            "/world/{world_name}/permissions/access/communities/{communityId}",
        )
        .path(&format!(
            "/world/{}/permissions/access/communities/%20",
            WORLD
        ))
        .signed(&owner)
        .expect(400),
    )
    .await;

    let comms_path = format!("/worlds/{}/comms", WORLD);
    gate.hit(
        &app,
        Case::new("post", "/worlds/{world_name}/comms")
            .path(&comms_path)
            .signed(&owner),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/worlds/{world_name}/comms")
            .path(&comms_path)
            .expect(400),
    )
    .await;

    let scene_comms_path = format!("/worlds/{}/scenes/{}/comms", WORLD, scene_id);
    gate.hit(
        &app,
        Case::new("post", "/worlds/{world_name}/scenes/{scene_id}/comms")
            .path(&scene_comms_path)
            .signed(&owner),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/worlds/{world_name}/scenes/{scene_id}/comms")
            .path(&format!("/worlds/{}/scenes/nope/comms", WORLD))
            .signed(&owner)
            .expect(404),
    )
    .await;

    let join_body = serde_json::to_vec(&json!({
        "event": "participant_joined",
        "room": { "name": world_room_name(WORLD) },
        "participant": { "identity": owner.address() }
    }))
    .unwrap();
    let jwt = webhook_jwt(&join_body, LIVEKIT_SECRET);
    gate.hit(
        &app,
        Case::new("post", "/livekit-webhook")
            .header("authorization", &jwt)
            .body(join_body.clone(), "application/webhook+json"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/livekit-webhook")
            .body(b"{}".to_vec(), "application/webhook+json")
            .expect(400),
    )
    .await;
    // A well-formed token signed with the wrong secret. Pinned separately from the
    // missing-header 400 because it is the branch that decides whether anything that
    // reaches this route is LiveKit, and an unpinned 401 is how a fixture drifts into
    // asserting nothing.
    let forged = webhook_jwt(&join_body, "not-the-livekit-secret");
    gate.hit(
        &app,
        Case::new("post", "/livekit-webhook")
            .header("authorization", &forged)
            .body(join_body, "application/webhook+json")
            .expect(401),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/wallet/{wallet}/connected-world")
            .path(&format!("/wallet/{}/connected-world", owner.address())),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/wallet/{wallet}/connected-world")
            .path(&format!("/wallet/{}/connected-world", stranger.address()))
            .expect(404),
    )
    .await;

    let missing_path = format!(
        "/contents/{}",
        catalyrst_hashing::hash_bytes_v1(b"cg-missing-content")
    );
    gate.hit(
        &app,
        Case::new("get", "/contents/{hash}").path(&format!("/contents/{}", thumb_hash)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/contents/{hash}")
            .path(&missing_path)
            .expect(500),
    )
    .await;
    gate.hit(
        &app,
        Case::new("head", "/contents/{hash}").path(&format!("/contents/{}", thumb_hash)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("head", "/contents/{hash}")
            .path(&missing_path)
            .expect(500),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/available-content").query(&format!("cid={}", thumb_hash)),
    )
    .await;
    gate.waive_error(
        "get",
        "/available-content",
        "missing or unknown cids answer 200 with available=false; the documented 400 is unreachable",
    );

    gate.hit(&app, Case::new("get", "/admin/worlds").bearer(ADMIN_TOKEN))
        .await;
    gate.hit(&app, Case::new("get", "/admin/worlds").expect(403))
        .await;
    gate.hit(
        &app,
        Case::new("get", "/admin/worlds/{world_name}")
            .path(&format!("/admin/worlds/{}", WORLD))
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/admin/worlds/{world_name}")
            .path("/admin/worlds/nope.dcl.eth")
            .bearer(ADMIN_TOKEN)
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/admin/worlds/{world_name}/ban-status")
            .path(&format!("/admin/worlds/{}/ban-status", WORLD))
            .query(&format!("address={}", owner.address()))
            .bearer(ADMIN_TOKEN)
            .expect(503),
    )
    .await;
    gate.waive_success(
        "get",
        "/admin/worlds/{world_name}/ban-status",
        "the route proxies the comms-gatekeeper; 200 needs a live gatekeeper, unconfigured deployments answer 503",
    );
    gate.hit(
        &app,
        Case::new("get", "/admin/worlds/{world_name}/ban-status")
            .path(&format!("/admin/worlds/{}/ban-status", WORLD))
            .query(&format!("address={}", owner.address()))
            .expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/worlds/{world_name}/disable")
            .path(&format!("/admin/worlds/{}/disable", WORLD))
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/worlds/{world_name}/disable")
            .path("/admin/worlds/nope.dcl.eth/disable")
            .bearer(ADMIN_TOKEN)
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/worlds/{world_name}/enable")
            .path(&format!("/admin/worlds/{}/enable", WORLD))
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/worlds/{world_name}/enable")
            .path("/admin/worlds/nope.dcl.eth/enable")
            .bearer(ADMIN_TOKEN)
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/blocked/{wallet}")
            .path(&format!("/admin/blocked/{}", stranger.address()))
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/blocked/{wallet}")
            .path(&format!("/admin/blocked/{}", stranger.address()))
            .expect(403),
    )
    .await;
    gate.hit(&app, Case::new("get", "/admin/blocked").bearer(ADMIN_TOKEN))
        .await;
    gate.hit(&app, Case::new("get", "/admin/blocked").expect(403))
        .await;
    gate.hit(
        &app,
        Case::new("delete", "/admin/blocked/{wallet}")
            .path(&format!("/admin/blocked/{}", stranger.address()))
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/admin/blocked/{wallet}")
            .path(&format!("/admin/blocked/{}", stranger.address()))
            .bearer(ADMIN_TOKEN)
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/admin/access-log").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(&app, Case::new("get", "/admin/access-log").expect(403))
        .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/gc")
            .query("dry_run=true")
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(&app, Case::new("post", "/admin/gc").expect(403))
        .await;

    gate.hit(
        &app,
        Case::new("delete", "/world/{world_name}/scenes/{scene_coord}")
            .path(&format!("/world/{}/scenes/0,0", WORLD))
            .signed(&outsider)
            .expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/world/{world_name}/scenes/{scene_coord}")
            .path(&format!("/world/{}/scenes/0,0", WORLD))
            .signed(&owner),
    )
    .await;

    gate.hit(
        &app,
        Case::new("delete", "/entities/{world_name}")
            .path(&format!("/entities/{}", WORLD))
            .signed(&outsider)
            .expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/entities/{world_name}")
            .path(&format!("/entities/{}", WORLD))
            .signed(&owner),
    )
    .await;

    // --- federation ---------------------------------------------------------
    //
    // Ordered: refresh first, because it is what puts a row in the mirror, and the
    // mirror listing and the veto both need one to answer 200 over a non-empty body.

    // Unauthenticated first, so the 403 is recorded before the fixture ever holds a
    // populated mirror -- an admin route whose refusal is only ever observed after the
    // happy path has run is a refusal nobody watched.
    gate.hit(
        &fed_app,
        Case::new("post", "/admin/federation/worlds/refresh").expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/admin/federation/worlds/refresh")
            .bearer(ADMIN_TOKEN)
            .expect(503),
    )
    .await;
    let refreshed = gate
        .hit(
            &fed_app,
            Case::new("post", "/admin/federation/worlds/refresh").bearer(ADMIN_TOKEN),
        )
        .await;
    assert_eq!(
        refreshed["polled"][0]["ok"], true,
        "the stub peer must have been polled successfully, or the 200 bodies below \
         are schema-valid over nothing: {refreshed}"
    );

    gate.hit(&fed_app, Case::new("get", "/federation/worlds/peers"))
        .await;
    gate.hit(
        &app,
        Case::new("get", "/federation/worlds/peers").expect(503),
    )
    .await;

    let mirror = gate
        .hit(&fed_app, Case::new("get", "/federation/worlds/mirror"))
        .await;
    assert_eq!(
        mirror["total"], 1,
        "the mirror listing must be non-empty, or its schema was validated over an \
         empty array: {mirror}"
    );
    gate.hit(
        &fed_app,
        Case::new("get", "/federation/worlds/mirror")
            .query("peer=not-an-admitted-peer.example.org")
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/federation/worlds/mirror").expect(503),
    )
    .await;

    let hidden_path = format!(
        "/admin/federation/worlds/{}/{}/hidden",
        FED_PEER_ID, FED_PEER_WORLD
    );
    gate.hit(
        &fed_app,
        Case::new(
            "put",
            "/admin/federation/worlds/{peer_id}/{world_name}/hidden",
        )
        .path(&hidden_path)
        .expect(403)
        .json(&json!({ "hidden": true })),
    )
    .await;
    gate.hit(
        &fed_app,
        Case::new(
            "put",
            "/admin/federation/worlds/{peer_id}/{world_name}/hidden",
        )
        .path(&hidden_path)
        .bearer(ADMIN_TOKEN)
        .json(&json!({ "hidden": true })),
    )
    .await;
    gate.hit(
        &fed_app,
        Case::new(
            "put",
            "/admin/federation/worlds/{peer_id}/{world_name}/hidden",
        )
        .path(&format!(
            "/admin/federation/worlds/not-an-admitted-peer.example.org/{}/hidden",
            FED_PEER_WORLD
        ))
        .bearer(ADMIN_TOKEN)
        .json(&json!({ "hidden": true }))
        .expect(404),
    )
    .await;
    // `~` is a legal URI character, so this reaches the handler unmangled and is
    // refused on its shape rather than by the router.
    gate.hit(
        &fed_app,
        Case::new(
            "put",
            "/admin/federation/worlds/{peer_id}/{world_name}/hidden",
        )
        .path(&format!(
            "/admin/federation/worlds/{}/bad~name/hidden",
            FED_PEER_ID
        ))
        .bearer(ADMIN_TOKEN)
        .json(&json!({ "hidden": true }))
        .expect(400),
    )
    .await;

    gate.assert_covered();

    let _ = std::fs::remove_dir_all(&contents_dir);
    scratch.drop().await;
}
