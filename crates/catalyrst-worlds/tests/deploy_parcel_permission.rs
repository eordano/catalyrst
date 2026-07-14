//! HTTP-level cover for upstream #521's authorization rules:
//!
//! * a parcel-scoped `deployment` grant authorizes only the parcels attached to it (before
//!   this, deploy consulted `get_permission_records`, which never joined
//!   `world_permission_parcels`, so ANY grant was world-wide for deployment purposes);
//! * replacing a scene requires authority over that scene's COMPLETE footprint, on both the
//!   deploy and the unpublish path;
//! * a scene may not declare a base parcel outside its own parcels.

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
use serde_json::{json, Value};
use sqlx::PgPool;

const WORLD: &str = "parcelperm.dcl.eth";
const LABEL: &str = "parcelperm";

fn test_config(contents_dir: std::path::PathBuf) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 5146,
        database_url: "unused".into(),
        http_base_url: "http://parcelperm.test".into(),
        network_id: 1,
        squid_database_url: None,
        global_scenes_urn: None,
        content_public_url: "http://parcelperm.test/content".into(),
        lambdas_public_url: "http://parcelperm.test/lambdas".into(),
        livekit_host: "livekit.parcelperm.test".into(),
        livekit_ws_url: "wss://livekit.parcelperm.test".into(),
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
        admin_token: Some("parcelperm-admin".into()),
        max_in_flight_upload_bytes: 512 * 1024 * 1024,
        max_concurrent_uploads: catalyrst_worlds::upload_limits::DEFAULT_MAX_CONCURRENT_UPLOADS,
        max_in_flight_upload_files:
            catalyrst_worlds::upload_limits::DEFAULT_MAX_IN_FLIGHT_UPLOAD_FILES,
        multipart_upload_timeout_ms:
            catalyrst_worlds::upload_limits::DEFAULT_MULTIPART_UPLOAD_TIMEOUT_MS,
        deployment_processing_timeout_ms:
            catalyrst_worlds::upload_limits::DEFAULT_DEPLOYMENT_PROCESSING_TIMEOUT_MS,
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

async fn apply_migrations(scratch: &ScratchDb) {
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
}

async fn set_name_owner(pool: &PgPool, owner: &str) {
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
        .bind("ens-parcelperm")
        .bind(LABEL)
        .bind("0xbe92b49aee993adea3a002adcda189a2b7dec56c-ETHEREUM")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO squid_marketplace.nft (id, ens_id, category, owner_id) VALUES ($1, $2, 'ens', $3)",
    )
    .bind("nft-ens-parcelperm")
    .bind("ens-parcelperm")
    .bind(format!("{owner}-ETHEREUM"))
    .execute(pool)
    .await
    .unwrap();
}

/// A parcel-scoped `deployment` grant, written the way the permissions write surface writes
/// it: one `world_permissions` row plus one `world_permission_parcels` row per parcel.
async fn grant_parcels(pool: &PgPool, address: &str, parcels: &[&str]) {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO world_permissions (world_name, permission_type, address, created_at, updated_at)
         VALUES (lower($1), 'deployment', lower($2), now(), now())
         ON CONFLICT (world_name, permission_type, address) DO UPDATE SET updated_at = now()
         RETURNING id",
    )
    .bind(WORLD)
    .bind(address)
    .fetch_one(pool)
    .await
    .unwrap();
    for parcel in parcels {
        sqlx::query(
            "INSERT INTO world_permission_parcels (permission_id, parcel) VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(*parcel)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn deployed_entity_ids(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT entity_id FROM world_scenes WHERE lower(world_name) = lower($1) ORDER BY entity_id",
    )
    .bind(WORLD)
    .fetch_all(pool)
    .await
    .unwrap()
}

struct Deployment {
    body: Vec<u8>,
    content_type: String,
    entity_id: String,
}

fn deployment(wallet: &Wallet, title: &str, base: &str, pointers: &[&str]) -> Deployment {
    let thumb = title.as_bytes().to_vec();
    let thumb_hash = catalyrst_hashing::hash_bytes_v1(&thumb);
    let entity = json!({
        "type": "scene",
        "timestamp": chrono::Utc::now().timestamp_millis(),
        "pointers": pointers,
        "content": [{ "file": "thumb.png", "hash": thumb_hash }],
        "metadata": {
            "display": { "title": title, "navmapThumbnail": "thumb.png" },
            "worldConfiguration": { "name": WORLD },
            "scene": { "base": base, "parcels": pointers }
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
    Deployment {
        body,
        content_type,
        entity_id,
    }
}

async fn post_entity(gate: &mut Gate, app: &Router, deployment: &Deployment, expect: u16) -> Value {
    gate.hit(
        app,
        Case::new("post", "/entities")
            .body(deployment.body.clone(), &deployment.content_type)
            .expect(expect),
    )
    .await
}

struct Fixture {
    scratch: ScratchDb,
    app: Router,
    gate: Gate,
    contents_dir: std::path::PathBuf,
}

async fn fixture(tag: &str, owner: &Wallet) -> Option<Fixture> {
    let scratch = ScratchDb::create("CATALYRST_WORLDS_TEST_PG", tag).await?;
    apply_migrations(&scratch).await;
    set_name_owner(&scratch.pool, &owner.address().to_lowercase()).await;

    let contents_dir = std::env::temp_dir().join(format!("{tag}-{}", scratch.database));
    std::fs::create_dir_all(&contents_dir).unwrap();
    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone(), contents_dir.clone());
    let app: Router = router.with_state(state);
    let gate = Gate::new(serde_json::to_value(&spec).unwrap());
    Some(Fixture {
        scratch,
        app,
        gate,
        contents_dir,
    })
}

impl Fixture {
    async fn finish(self) {
        let _ = std::fs::remove_dir_all(&self.contents_dir);
        self.scratch.drop().await;
    }
}

#[tokio::test]
async fn a_parcel_scoped_grant_authorizes_only_its_own_parcels() {
    let owner = test_wallet(41);
    let grantee = test_wallet(42);
    let Some(mut f) = fixture("pp_scope", &owner).await else {
        return;
    };

    // Seed the world (and the owner row) from a parcel nothing else touches.
    let seed = deployment(&owner, "Seed", "9,9", &["9,9"]);
    post_entity(&mut f.gate, &f.app, &seed, 200).await;

    grant_parcels(&f.scratch.pool, &grantee.address().to_lowercase(), &["0,0"]).await;

    // "0,1" is outside the grant. Before #521 the deployment check never looked at
    // `world_permission_parcels` at all, so this landed.
    let outside = deployment(&grantee, "Outside", "0,0", &["0,0", "0,1"]);
    post_entity(&mut f.gate, &f.app, &outside, 403).await;
    assert!(
        !deployed_entity_ids(&f.scratch.pool)
            .await
            .contains(&outside.entity_id),
        "a parcel-scoped grant must not authorize parcels outside it"
    );

    // The same wallet, confined to its own parcel, still deploys.
    let inside = deployment(&grantee, "Inside", "0,0", &["0,0"]);
    post_entity(&mut f.gate, &f.app, &inside, 200).await;
    assert!(deployed_entity_ids(&f.scratch.pool)
        .await
        .contains(&inside.entity_id));

    f.finish().await;
}

#[tokio::test]
async fn replacing_a_scene_requires_authority_over_its_whole_footprint() {
    let owner = test_wallet(43);
    let grantee = test_wallet(44);
    let Some(mut f) = fixture("pp_replace", &owner).await else {
        return;
    };

    let existing = deployment(&owner, "Existing", "0,0", &["0,0", "0,1"]);
    post_entity(&mut f.gate, &f.app, &existing, 200).await;
    let untouched = deployment(&owner, "Untouched", "7,7", &["7,7"]);
    post_entity(&mut f.gate, &f.app, &untouched, 200).await;

    grant_parcels(&f.scratch.pool, &grantee.address().to_lowercase(), &["0,0"]).await;

    // The pointers are covered by the grant, but deploying here would DELETE a scene that
    // also occupies "0,1" -- which the grantee has no say over.
    let partial = deployment(&grantee, "Partial", "0,0", &["0,0"]);
    post_entity(&mut f.gate, &f.app, &partial, 403).await;
    let mut ids = deployed_entity_ids(&f.scratch.pool).await;
    ids.sort();
    let mut expected = vec![existing.entity_id.clone(), untouched.entity_id.clone()];
    expected.sort();
    assert_eq!(ids, expected, "the 403 must leave both scenes in place");

    // Widen the grant to the replaced scene's whole footprint and the same deploy lands.
    grant_parcels(
        &f.scratch.pool,
        &grantee.address().to_lowercase(),
        &["0,0", "0,1"],
    )
    .await;
    let full = deployment(&grantee, "Full", "0,0", &["0,0"]);
    post_entity(&mut f.gate, &f.app, &full, 200).await;

    let mut ids = deployed_entity_ids(&f.scratch.pool).await;
    ids.sort();
    let mut expected = vec![full.entity_id.clone(), untouched.entity_id.clone()];
    expected.sort();
    assert_eq!(
        ids, expected,
        "only the authorized scene identity may be replaced"
    );

    f.finish().await;
}

#[tokio::test]
async fn a_scene_may_not_declare_a_base_parcel_outside_itself() {
    let owner = test_wallet(45);
    let Some(mut f) = fixture("pp_base", &owner).await else {
        return;
    };

    let liar = deployment(&owner, "Liar", "50,50", &["0,0", "0,1"]);
    let body = post_entity(&mut f.gate, &f.app, &liar, 400).await;
    let errors = body["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e.as_str()
            == Some(
                "The scene base parcel [50,50] must be included in the scene parcels [0,0, 0,1]."
            )),
        "expected the base-not-in-parcels error, got {errors:?}"
    );
    assert!(deployed_entity_ids(&f.scratch.pool).await.is_empty());

    let honest = deployment(&owner, "Honest", "0,1", &["0,0", "0,1"]);
    post_entity(&mut f.gate, &f.app, &honest, 200).await;

    f.finish().await;
}

#[tokio::test]
async fn unpublishing_requires_authority_over_the_whole_scene_footprint() {
    let owner = test_wallet(46);
    let grantee = test_wallet(47);
    let Some(mut f) = fixture("pp_unpublish", &owner).await else {
        return;
    };

    let wide = deployment(&owner, "Wide", "0,0", &["0,0", "0,1"]);
    post_entity(&mut f.gate, &f.app, &wide, 200).await;

    grant_parcels(&f.scratch.pool, &grantee.address().to_lowercase(), &["0,0"]).await;

    let path = format!("/world/{WORLD}/scenes/0,0");
    f.gate
        .hit(
            &f.app,
            Case::new("delete", "/world/{world_name}/scenes/{scene_coord}")
                .path(&path)
                .signed(&grantee)
                .expect(403),
        )
        .await;
    assert_eq!(
        deployed_entity_ids(&f.scratch.pool).await,
        vec![wide.entity_id.clone()],
        "a grant on one parcel must not unpublish a scene that spans two"
    );

    grant_parcels(
        &f.scratch.pool,
        &grantee.address().to_lowercase(),
        &["0,0", "0,1"],
    )
    .await;
    f.gate
        .hit(
            &f.app,
            Case::new("delete", "/world/{world_name}/scenes/{scene_coord}")
                .path(&path)
                .signed(&grantee)
                .expect(200),
        )
        .await;
    assert!(deployed_entity_ids(&f.scratch.pool).await.is_empty());

    f.finish().await;
}
