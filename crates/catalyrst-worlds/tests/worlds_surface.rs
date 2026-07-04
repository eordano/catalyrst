use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use catalyrst_worlds::access::AccessSetting;
use catalyrst_worlds::http::ApiError;
use catalyrst_worlds::ports::worlds::{
    OrderDirection, WorldsComponent, WorldsListFilters, WorldsListOptions, WorldsOrderBy,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_WORLDS_TEST_PG").ok()
}

fn unique_schema() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("test_worlds_{}_{}_{}", std::process::id(), nanos, n)
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()?;
    let schema = unique_schema();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
        .execute(&admin)
        .await
        .ok()?;
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;

    apply_migration(&pool, include_str!("../migrations/0001_init.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0002_access_log.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0003_permission_parcels.sql"),
    )
    .await;

    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_line_comments(sql);
    let mut buf = String::new();
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("migration stmt failed: {e}\n{}", buf.clone()));
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
            .execute(pool)
            .await
            .expect("trailing sql");
    }
}

fn strip_line_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        if line.trim_start().starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn deploy_entity(title: &str, thumb: &str) -> serde_json::Value {
    json!({
        "type": "scene",
        "timestamp": 1000,
        "pointers": ["0,0", "0,1"],
        "content": [{ "file": "thumb.png", "hash": thumb }],
        "metadata": {
            "display": { "title": title, "description": "A test world", "navmapThumbnail": "thumb.png" },
            "worldConfiguration": {
                "name": "test.dcl.eth",
                "skyboxConfig": { "fixedTime": 36000 },
                "fixedAdapter": "offline:offline"
            },
            "scene": { "base": "0,0", "parcels": ["0,0", "0,1"] },
            "tags": ["art", "game"],
            "rating": "E"
        }
    })
}

#[tokio::test]
async fn deploy_populates_settings_and_read_surfaces() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping deploy_populates_settings_and_read_surfaces: set CATALYRST_WORLDS_TEST_PG to run");
        return;
    };
    let wc = WorldsComponent::new(pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";

    wc.deploy_scene(
        "test.dcl.eth",
        owner,
        "bafyentity",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("My World", "bafythumb"),
        &["0,0".to_string(), "0,1".to_string()],
        123,
    )
    .await
    .expect("deploy_scene");

    let settings = wc
        .get_world_settings("test.dcl.eth")
        .await
        .unwrap()
        .expect("settings");
    assert_eq!(settings.title.as_deref(), Some("My World"));
    assert_eq!(settings.content_rating.as_deref(), Some("E"));
    assert_eq!(settings.skybox_time, Some(36000));
    assert_eq!(
        settings.categories.as_deref(),
        Some(&["art".to_string(), "game".to_string()][..])
    );
    assert_eq!(settings.single_player, Some(true));
    assert_eq!(settings.show_in_places, Some(true));
    assert_eq!(settings.thumbnail_hash.as_deref(), Some("bafythumb"));
    assert_eq!(settings.spawn_coordinates.as_deref(), Some("0,0"));

    let (worlds, total) = wc
        .list_worlds_public(
            &WorldsListFilters::default(),
            &WorldsListOptions {
                limit: 50,
                offset: 0,
                order_by: WorldsOrderBy::Name,
                order_direction: OrderDirection::Asc,
            },
        )
        .await
        .unwrap();
    assert_eq!(total, 1);
    let w = &worlds[0];
    assert_eq!(w.name, "test.dcl.eth");
    assert_eq!(w.deployed_scenes, 1);
    assert!(w.last_deployed_at.is_some());
    assert_eq!(
        (w.min_x, w.max_x, w.min_y, w.max_y),
        (Some(0), Some(0), Some(0), Some(1))
    );

    let (with_scenes, _) = wc
        .list_worlds_public(
            &WorldsListFilters {
                has_deployed_scenes: Some(true),
                ..Default::default()
            },
            &list_opts(),
        )
        .await
        .unwrap();
    assert_eq!(with_scenes.len(), 1);
    let (without_scenes, _) = wc
        .list_worlds_public(
            &WorldsListFilters {
                has_deployed_scenes: Some(false),
                ..Default::default()
            },
            &list_opts(),
        )
        .await
        .unwrap();
    assert!(without_scenes.is_empty());

    let (as_owner, _) = wc
        .list_worlds_public(
            &WorldsListFilters {
                authorized_deployer: Some(owner.to_lowercase()),
                ..Default::default()
            },
            &list_opts(),
        )
        .await
        .unwrap();
    assert_eq!(as_owner.len(), 1);
    let (as_stranger, _) = wc
        .list_worlds_public(
            &WorldsListFilters {
                authorized_deployer: Some("0x2222222222222222222222222222222222222222".into()),
                ..Default::default()
            },
            &list_opts(),
        )
        .await
        .unwrap();
    assert!(as_stranger.is_empty());

    let manifest = wc
        .get_world_manifest("test.dcl.eth")
        .await
        .unwrap()
        .expect("manifest");
    assert_eq!(manifest.total, 2);
    assert_eq!(manifest.parcels, vec!["0,0".to_string(), "0,1".to_string()]);
    assert_eq!(manifest.spawn_coordinates.as_deref(), Some("0,0"));

    cleanup(&admin_url, &schema).await;
}

fn list_opts() -> WorldsListOptions {
    WorldsListOptions {
        limit: 50,
        offset: 0,
        order_by: WorldsOrderBy::Name,
        order_direction: OrderDirection::Asc,
    }
}

#[tokio::test]
async fn permission_parcels_lifecycle() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping permission_parcels_lifecycle: set CATALYRST_WORLDS_TEST_PG to run");
        return;
    };
    let wc = WorldsComponent::new(pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";
    let deployer = "0xAAaAAaAAaAAAAaaAaaAaaaAaaAaAAaAaAaAAaAAA";
    let streamer = "0xBBbBBBbbBbBbBBBBBbBbBBbBbbBbbbBbbBBbBBBb";

    wc.create_basic_world_if_not_exists("perm.dcl.eth", owner)
        .await
        .unwrap();

    let added = wc
        .grant_addresses_world_wide_permission(
            "perm.dcl.eth",
            "deployment",
            &[deployer.to_string()],
        )
        .await
        .unwrap();
    assert_eq!(added, vec![deployer.to_lowercase()]);
    assert!(wc
        .has_world_wide_permission("perm.dcl.eth", "deployment", deployer)
        .await
        .unwrap());

    let created = wc
        .add_parcels_to_permission(
            "perm.dcl.eth",
            "streaming",
            streamer,
            &["00,00".into(), "1,1".into()],
        )
        .await
        .unwrap();
    assert!(created);

    let records = wc
        .get_world_permission_records_full("perm.dcl.eth")
        .await
        .unwrap();
    let dep = records
        .iter()
        .find(|r| r.permission_type == "deployment")
        .unwrap();
    assert!(dep.is_world_wide);
    assert_eq!(dep.parcel_count, 0);
    let strm = records
        .iter()
        .find(|r| r.permission_type == "streaming")
        .unwrap();
    assert!(!strm.is_world_wide);
    assert_eq!(strm.parcel_count, 2);
    assert!(!wc
        .has_world_wide_permission("perm.dcl.eth", "streaming", streamer)
        .await
        .unwrap());

    let perm_id = wc
        .get_address_permission_id("perm.dcl.eth", "streaming", streamer)
        .await
        .unwrap()
        .expect("streaming perm id");
    let (total, parcels) = wc
        .get_parcels_for_permission(perm_id, 100, 0, None)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(parcels, vec!["0,0".to_string(), "1,1".to_string()]);

    let (_bt, bparcels) = wc
        .get_parcels_for_permission(perm_id, 100, 0, Some((0, 0, 0, 0)))
        .await
        .unwrap();
    assert_eq!(bparcels, vec!["0,0".to_string()]);

    let (atot, addrs) = wc
        .get_addresses_for_parcel_permission("perm.dcl.eth", "streaming", &["0,0".into()], 100, 0)
        .await
        .unwrap();
    assert_eq!(atot, 1);
    assert_eq!(addrs, vec![streamer.to_lowercase()]);

    wc.remove_parcels_from_permission(perm_id, &["0,0".into(), "1,1".into()])
        .await
        .unwrap();
    let records = wc
        .get_world_permission_records_full("perm.dcl.eth")
        .await
        .unwrap();
    let strm = records
        .iter()
        .find(|r| r.permission_type == "streaming")
        .unwrap();
    assert!(strm.is_world_wide);
    assert!(wc
        .has_world_wide_permission("perm.dcl.eth", "streaming", streamer)
        .await
        .unwrap());

    let removed = wc
        .remove_addresses_permission("perm.dcl.eth", "deployment", &[deployer.to_string()])
        .await
        .unwrap();
    assert_eq!(removed, vec![deployer.to_lowercase()]);

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn access_allow_list_modify() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping access_allow_list_modify: set CATALYRST_WORLDS_TEST_PG to run");
        return;
    };
    let wc = WorldsComponent::new(pool.clone());

    wc.create_basic_world_if_not_exists(
        "acc.dcl.eth",
        "0x1111111111111111111111111111111111111111",
    )
    .await
    .unwrap();
    wc.store_access(
        "acc.dcl.eth",
        &AccessSetting::AllowList {
            wallets: vec![],
            communities: vec![],
        },
    )
    .await
    .unwrap();

    wc.modify_access_atomically("acc.dcl.eth", |access| match access {
        AccessSetting::AllowList {
            mut wallets,
            communities,
        } => {
            wallets.push("0xabc".into());
            Ok(AccessSetting::AllowList {
                wallets,
                communities,
            })
        }
        _ => Err(ApiError::bad_request("not allow-list")),
    })
    .await
    .unwrap();

    let updated = wc
        .modify_access_atomically("acc.dcl.eth", |access| match access {
            AccessSetting::AllowList {
                wallets,
                mut communities,
            } => {
                communities.push("community-1".into());
                Ok(AccessSetting::AllowList {
                    wallets,
                    communities,
                })
            }
            _ => Err(ApiError::bad_request("not allow-list")),
        })
        .await
        .unwrap();
    match updated {
        AccessSetting::AllowList {
            wallets,
            communities,
        } => {
            assert_eq!(wallets, vec!["0xabc".to_string()]);
            assert_eq!(communities, vec!["community-1".to_string()]);
        }
        _ => panic!("expected allow-list"),
    }

    let world = wc.get_world("acc.dcl.eth").await.unwrap().expect("world");
    match world.access {
        AccessSetting::AllowList {
            wallets,
            communities,
        } => {
            assert_eq!(wallets, vec!["0xabc".to_string()]);
            assert_eq!(communities, vec!["community-1".to_string()]);
        }
        _ => panic!("expected allow-list access"),
    }

    wc.store_access("acc.dcl.eth", &AccessSetting::Unrestricted)
        .await
        .unwrap();
    let err = wc
        .modify_access_atomically("acc.dcl.eth", |access| match access {
            AccessSetting::AllowList { .. } => Ok(access),
            _ => Err(ApiError::bad_request("not allow-list")),
        })
        .await;
    assert!(err.is_err());

    cleanup(&admin_url, &schema).await;
}
