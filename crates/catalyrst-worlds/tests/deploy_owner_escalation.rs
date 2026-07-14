use std::sync::Arc;

use axum::Router;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{
    create_simple_auth_chain, multipart_body, test_wallet, Case, Gate, MultipartPart, Wallet,
};
use catalyrst_worlds::config::Config;
use catalyrst_worlds::ports::bans::BansComponent;
use catalyrst_worlds::ports::denylist::DenyListComponent;
use catalyrst_worlds::ports::name_denylist::NameDenyListChecker;
use catalyrst_worlds::ports::presence::PeersRegistry;
use catalyrst_worlds::ports::worlds::WorldsComponent;
use catalyrst_worlds::rate_limiter::RateLimiter;
use catalyrst_worlds::{api_router_with_spec, AppState, AppStateInner};
use serde_json::json;
use sqlx::PgPool;

const WORLD: &str = "escalate.dcl.eth";
const LABEL: &str = "escalate";

fn test_config(contents_dir: std::path::PathBuf) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 5146,
        database_url: "unused".into(),
        http_base_url: "http://escalate.test".into(),
        network_id: 1,
        squid_database_url: None,
        global_scenes_urn: None,
        content_public_url: "http://escalate.test/content".into(),
        lambdas_public_url: "http://escalate.test/lambdas".into(),
        livekit_host: "livekit.escalate.test".into(),
        livekit_ws_url: "wss://livekit.escalate.test".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
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
        admin_token: Some("escalate-admin".into()),
        max_in_flight_upload_bytes: 512 * 1024 * 1024,
        max_concurrent_uploads: catalyrst_worlds::upload_limits::DEFAULT_MAX_CONCURRENT_UPLOADS,
        max_in_flight_upload_files:
            catalyrst_worlds::upload_limits::DEFAULT_MAX_IN_FLIGHT_UPLOAD_FILES,
        multipart_upload_timeout_ms:
            catalyrst_worlds::upload_limits::DEFAULT_MULTIPART_UPLOAD_TIMEOUT_MS,
        deployment_processing_timeout_ms:
            catalyrst_worlds::upload_limits::DEFAULT_DEPLOYMENT_PROCESSING_TIMEOUT_MS,
        // Federation off. These suites assert the *unfederated* behaviour of every
        // route they touch, and that behaviour must be identical with the mirror
        // compiled in -- which is exactly what leaving this at the default proves.
        federation: catalyrst_worlds::fed::config::WorldsFedConfig::default(),
    }
}

fn build_state(pool: PgPool, contents_dir: std::path::PathBuf) -> AppState {
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    Arc::new(AppStateInner {
        sfu: catalyrst_livekit::SfuHealth::always_alive(),
        cfg: test_config(contents_dir),
        worlds: WorldsComponent::new(pool.clone()),
        presence: PeersRegistry::new(),
        rate_limiter: RateLimiter::new(),
        bans: BansComponent::new(http.clone(), None, None),
        denylist: DenyListComponent::new(http.clone(), None),
        name_denylist: NameDenyListChecker::new(http.clone(), None),
        http,
        squid_pool: Some(pool.clone()),
        fed_peers: catalyrst_worlds::fed::peers::WorldsFederationPeers::NotConfigured,
        mirror: catalyrst_worlds::fed::poll::WorldsMirror::new(
            pool,
            catalyrst_worlds::fed::config::WorldsFedConfig::default(),
            &catalyrst_worlds::fed::peers::WorldsFederationPeers::NotConfigured,
        ),
    })
}

// Mirrors the squid's real shape: `ens.owner_id` holds the registrar caller and
// is NOT the owner, while `nft.owner_id` is the ERC-721 owner. The fixture keeps
// them distinct so a query reading the wrong column fails these tests.
const ENS_ROW_ID: &str = "ens-0xregistrar-1";
const REGISTRAR_CALLER: &str = "0xbe92b49aee993adea3a002adcda189a2b7dec56c-ETHEREUM";

async fn create_ens_table(pool: &PgPool) {
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
}

async fn set_ens_owner(pool: &PgPool, owner: &str) {
    sqlx::query("INSERT INTO squid_marketplace.ens (id, subdomain, owner_id) VALUES ($1, $2, $3)")
        .bind(ENS_ROW_ID)
        .bind(LABEL)
        .bind(REGISTRAR_CALLER)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO squid_marketplace.nft (id, ens_id, category, owner_id) VALUES ($1, $2, 'ens', $3)",
    )
    .bind(format!("nft-{ENS_ROW_ID}"))
    .bind(ENS_ROW_ID)
    .bind(format!("{}-ETHEREUM", owner))
    .execute(pool)
    .await
    .unwrap();
}

async fn forget_ens_owner(pool: &PgPool) {
    sqlx::query("DELETE FROM squid_marketplace.nft WHERE ens_id = $1")
        .bind(ENS_ROW_ID)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM squid_marketplace.ens WHERE subdomain = $1")
        .bind(LABEL)
        .execute(pool)
        .await
        .unwrap();
}

async fn stored_owner(pool: &PgPool) -> Option<String> {
    sqlx::query_scalar("SELECT owner FROM worlds WHERE lower(name) = lower($1)")
        .bind(WORLD)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn deploy_multipart(wallet: &Wallet, title: &str) -> (Vec<u8>, String) {
    let thumb = title.as_bytes().to_vec();
    let thumb_hash = catalyrst_hashing::hash_bytes_v1(&thumb);
    let entity = json!({
        "type": "scene",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "pointers": ["0,0", "0,1"],
        "content": [{ "file": "thumb.png", "hash": thumb_hash }],
        "metadata": {
            "display": { "title": title, "navmapThumbnail": "thumb.png" },
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
        MultipartPart::file("thumb.png", "thumb.png", "image/png", thumb),
    ]);
    (body, content_type)
}

#[tokio::test]
async fn acl_deployer_cannot_take_over_an_existing_world_owner() {
    let Some(scratch) = ScratchDb::create("CATALYRST_WORLDS_TEST_PG", "esc_worlds").await else {
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

    let name_owner = test_wallet(21);
    let acl_deployer = test_wallet(22);
    let name_owner_address = name_owner.address().to_lowercase();
    let acl_deployer_address = acl_deployer.address().to_lowercase();

    create_ens_table(&scratch.pool).await;
    set_ens_owner(&scratch.pool, &name_owner_address).await;

    let contents_dir = std::env::temp_dir().join(format!("esc-worlds-{}", scratch.database));
    std::fs::create_dir_all(&contents_dir).unwrap();
    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone(), contents_dir.clone());
    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    let (body, content_type) = deploy_multipart(&name_owner, "Owner World");
    gate.hit(
        &app,
        Case::new("post", "/entities").body(body, &content_type),
    )
    .await;
    assert_eq!(
        stored_owner(&scratch.pool).await.as_deref(),
        Some(name_owner_address.as_str())
    );

    gate.hit(
        &app,
        Case::new("post", "/world/{world_name}/permissions/{permission_name}")
            .path(&format!("/world/{}/permissions/deployment", WORLD))
            .signed_meta(
                &name_owner,
                &json!({ "type": "allow-list", "wallets": [acl_deployer.address()] }),
            )
            .expect(204),
    )
    .await;

    forget_ens_owner(&scratch.pool).await;

    let (body, content_type) = deploy_multipart(&acl_deployer, "ACL World");
    gate.hit(
        &app,
        Case::new("post", "/entities").body(body, &content_type),
    )
    .await;

    assert_eq!(
        stored_owner(&scratch.pool).await.as_deref(),
        Some(name_owner_address.as_str()),
        "an ACL deployer rewrote worlds.owner to {acl_deployer_address}"
    );

    let permissions = gate
        .hit(
            &app,
            Case::new("get", "/world/{world_name}/permissions")
                .path(&format!("/world/{}/permissions", WORLD)),
        )
        .await;
    assert_eq!(
        permissions["owner"].as_str(),
        Some(name_owner_address.as_str())
    );

    gate.hit(
        &app,
        Case::new("post", "/world/{world_name}/permissions/{permission_name}")
            .path(&format!("/world/{}/permissions/deployment", WORLD))
            .signed_meta(
                &acl_deployer,
                &json!({ "type": "allow-list", "wallets": [acl_deployer.address()] }),
            )
            .expect(403),
    )
    .await;

    let _ = std::fs::remove_dir_all(&contents_dir);
    scratch.drop().await;
}

#[tokio::test]
async fn first_deploy_seeds_the_resolved_name_owner() {
    let Some(scratch) = ScratchDb::create("CATALYRST_WORLDS_TEST_PG", "esc_seed").await else {
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

    let name_owner = test_wallet(23);
    let name_owner_address = name_owner.address().to_lowercase();
    create_ens_table(&scratch.pool).await;
    set_ens_owner(&scratch.pool, &name_owner_address).await;

    let contents_dir = std::env::temp_dir().join(format!("esc-seed-{}", scratch.database));
    std::fs::create_dir_all(&contents_dir).unwrap();
    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone(), contents_dir.clone());
    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    let (body, content_type) = deploy_multipart(&name_owner, "Seeded World");
    gate.hit(
        &app,
        Case::new("post", "/entities").body(body, &content_type),
    )
    .await;
    assert_eq!(
        stored_owner(&scratch.pool).await.as_deref(),
        Some(name_owner_address.as_str())
    );

    let _ = std::fs::remove_dir_all(&contents_dir);
    scratch.drop().await;
}
