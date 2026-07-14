use std::sync::Arc;

use axum::Router;
use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{
    create_simple_auth_chain, multipart_body, test_wallet, Case, Gate, MultipartPart, Wallet,
};
use catalyrst_worlds::config::Config;
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

async fn squid_fixture(pool: &PgPool, owner: &str) {
    sqlx::query("CREATE SCHEMA squid_marketplace")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE squid_marketplace.ens (subdomain text PRIMARY KEY, owner_id text NOT NULL)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO squid_marketplace.ens (subdomain, owner_id) VALUES ($1, $2)")
        .bind("gate")
        .bind(format!("{}-ETHEREUM", owner))
        .execute(pool)
        .await
        .unwrap();
}

fn test_config(contents_dir: std::path::PathBuf) -> Config {
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
        contents_upstream_url: "http://127.0.0.1:9".into(),
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
    }
}

fn build_state(pool: PgPool, contents_dir: std::path::PathBuf) -> AppState {
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    Arc::new(AppStateInner {
        cfg: test_config(contents_dir),
        worlds: WorldsComponent::new(pool.clone()),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(),
        bans: BansComponent::new(http.clone(), None, None),
        denylist: DenyListComponent::new(http.clone(), None),
        name_denylist: NameDenyListChecker::new(http.clone(), None),
        http,
        squid_pool: Some(pool),
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

fn webhook_jwt(body: &[u8]) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims = json!({
        "iss": LIVEKIT_KEY,
        "sha256": B64_STANDARD.encode(Sha256::digest(body)),
    });
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    let signing_input = format!("{}.{}", header, payload);
    let mut mac = Hmac::<Sha256>::new_from_slice(LIVEKIT_SECRET.as_bytes()).unwrap();
    mac.update(signing_input.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{}.{}", signing_input, sig)
}

#[tokio::test]
async fn every_spec_route_answers_its_contract() {
    let Some(scratch) = ScratchDb::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds").await else {
        eprintln!("skipping worlds contract gate: set CATALYRST_WORLDS_TEST_PG to run");
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

    let owner = test_wallet(7);
    let stranger = test_wallet(9);
    let outsider = test_wallet(13);
    squid_fixture(&scratch.pool, &owner.address().to_lowercase()).await;

    let contents_dir = std::env::temp_dir().join(format!("cg-worlds-{}", scratch.database));
    std::fs::create_dir_all(&contents_dir).unwrap();
    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone(), contents_dir.clone());
    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    gate.hit(&app, Case::new("get", "/status")).await;
    gate.hit(&app, Case::new("get", "/live-data")).await;

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
    let jwt = webhook_jwt(&join_body);
    gate.hit(
        &app,
        Case::new("post", "/livekit-webhook")
            .header("authorization", &jwt)
            .body(join_body, "application/webhook+json"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/livekit-webhook")
            .body(b"{}".to_vec(), "application/webhook+json")
            .expect(400),
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

    gate.assert_covered();

    let _ = std::fs::remove_dir_all(&contents_dir);
    scratch.drop().await;
}
