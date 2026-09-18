use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_worlds::access::AccessSetting;
use catalyrst_worlds::http::ApiError;
use catalyrst_worlds::ports::worlds::{
    AllowListEdit, AllowListEditOutcome, OrderDirection, SceneReplacement, WorldsComponent,
    WorldsListFilters, WorldsListOptions, WorldsOrderBy,
};
use serde_json::json;

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLDS_TEST_PG", "cg_worlds_surface").await?;
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
    Some(scratch)
}

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

fn scratch_contents_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "worlds-surface-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn deploy_entity(title: &str, thumb: &str) -> serde_json::Value {
    deploy_entity_at(title, thumb, "0,0", &["0,0", "0,1"])
}

fn deploy_entity_at(title: &str, thumb: &str, base: &str, parcels: &[&str]) -> serde_json::Value {
    json!({
        "type": "scene",
        "timestamp": 1000,
        "pointers": parcels,
        "content": [{ "file": "thumb.png", "hash": thumb }],
        "metadata": {
            "display": { "title": title, "description": "A test world", "navmapThumbnail": "thumb.png" },
            "worldConfiguration": {
                "name": "test.dcl.eth",
                "skyboxConfig": { "fixedTime": 36000 },
                "fixedAdapter": "offline:offline"
            },
            "scene": { "base": base, "parcels": parcels },
            "tags": ["art", "game"],
            "rating": "E"
        }
    })
}

#[tokio::test]
async fn deploy_populates_settings_and_read_surfaces() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";
    let contents_dir = scratch_contents_dir("surface");
    std::fs::write(contents_dir.join("bafythumb"), PNG_MAGIC).unwrap();

    wc.deploy_scene(
        "test.dcl.eth",
        Some(owner),
        "bafyentity",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("My World", "bafythumb"),
        &["0,0".to_string(), "0,1".to_string()],
        123,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
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
    assert_eq!(
        settings.show_in_places, None,
        "no placesConfig.optOut declared, so the scene expressed nothing"
    );
    assert_eq!(settings.thumbnail_hash.as_deref(), Some("bafythumb"));
    assert_eq!(settings.spawn_coordinates.as_deref(), Some("0,0"));
    assert_eq!(settings.settings_version, 0);
    assert_eq!(settings.access_type.as_deref(), Some("unrestricted"));

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
    assert!(w.single_player);
    assert!(
        w.show_in_places,
        "an unexpressed show_in_places must list as its effective default"
    );
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

    scratch.drop().await;
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
async fn redeploy_refreshes_settings_only_when_scene_ends_up_sole_occupant() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x4444444444444444444444444444444444444444";
    let contents_dir = scratch_contents_dir("refresh");
    let auth = json!([{ "type": "SIGNER", "payload": owner }]);

    wc.deploy_scene(
        "refresh.dcl.eth",
        Some(owner),
        "bafyfirst",
        owner,
        &auth,
        &deploy_entity_at("First Title", "bafymissing", "0,0", &["0,0", "0,1"]),
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("first deploy");

    wc.deploy_scene(
        "refresh.dcl.eth",
        Some(owner),
        "bafyaside",
        owner,
        &auth,
        &deploy_entity_at("Aside Title", "bafymissing", "5,5", &["5,5"]),
        &["5,5".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("non-overlapping deploy");

    let settings = wc
        .get_world_settings("refresh.dcl.eth")
        .await
        .unwrap()
        .expect("settings");
    assert_eq!(
        settings.title.as_deref(),
        Some("First Title"),
        "a deploy leaving another scene in place must not refresh settings"
    );
    assert_eq!(settings.settings_version, 0);

    wc.deploy_scene(
        "refresh.dcl.eth",
        Some(owner),
        "bafytakeover",
        owner,
        &auth,
        &deploy_entity_at(
            "Takeover Title",
            "bafymissing",
            "0,0",
            &["0,0", "0,1", "5,5"],
        ),
        &["0,0".to_string(), "0,1".to_string(), "5,5".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("replacing deploy");

    let settings = wc
        .get_world_settings("refresh.dcl.eth")
        .await
        .unwrap()
        .expect("settings");
    assert_eq!(
        settings.title.as_deref(),
        Some("Takeover Title"),
        "a deploy replacing every deployed scene must refresh settings"
    );
    assert_eq!(settings.settings_version, 1);

    scratch.drop().await;
}

#[tokio::test]
async fn redeploy_preserves_owner_settings_the_scene_does_not_express() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x5555555555555555555555555555555555555555";
    let contents_dir = scratch_contents_dir("preserve");
    let auth = json!([{ "type": "SIGNER", "payload": owner }]);

    wc.deploy_scene(
        "preserve.dcl.eth",
        Some(owner),
        "bafyseed",
        owner,
        &auth,
        &deploy_entity("Seed Title", "bafymissing"),
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("first deploy");

    let bare = json!({
        "type": "scene",
        "timestamp": 2000,
        "pointers": ["0,0", "0,1"],
        "metadata": { "scene": { "base": "0,0", "parcels": ["0,0", "0,1"] } }
    });
    wc.deploy_scene(
        "preserve.dcl.eth",
        Some(owner),
        "bafybare",
        owner,
        &auth,
        &bare,
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("bare redeploy");

    let settings = wc
        .get_world_settings("preserve.dcl.eth")
        .await
        .unwrap()
        .expect("settings");
    assert_eq!(
        settings.title.as_deref(),
        Some("Seed Title"),
        "a field the redeploying scene does not declare must keep its stored value"
    );
    assert_eq!(settings.content_rating.as_deref(), Some("E"));
    assert_eq!(settings.single_player, Some(true));
    assert_eq!(
        settings.settings_version, 0,
        "a refresh that changes nothing must not bump the settings version"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn deploy_rejects_out_of_policy_scene_metadata() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x6666666666666666666666666666666666666666";
    let contents_dir = scratch_contents_dir("policy");
    std::fs::write(contents_dir.join("bafynotimage"), b"<svg xmlns=\"a\"/>").unwrap();

    let entity = json!({
        "type": "scene",
        "timestamp": 1000,
        "pointers": ["0,0"],
        "content": [{ "file": "thumb.png", "hash": "bafynotimage" }],
        "metadata": {
            "display": { "title": "ab", "description": "xy", "navmapThumbnail": "thumb.png" },
            "worldConfiguration": {
                "name": "policy.dcl.eth",
                "skyboxConfig": { "fixedTime": 99999999999i64 }
            },
            "scene": { "base": "0,0", "parcels": ["0,0"] },
            "tags": ["ok", 7],
            "rating": "XXX"
        }
    });
    wc.deploy_scene(
        "policy.dcl.eth",
        Some(owner),
        "bafypolicy",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &entity,
        &["0,0".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy");

    let settings = wc
        .get_world_settings("policy.dcl.eth")
        .await
        .unwrap()
        .expect("settings");
    assert_eq!(settings.title, None, "a 2-char title is out of policy");
    assert_eq!(
        settings.description, None,
        "a 2-char description is out of policy"
    );
    assert_eq!(
        settings.content_rating, None,
        "an unknown rating is out of policy"
    );
    assert_eq!(
        settings.skybox_time, None,
        "an over-i32 fixedTime must not wrap"
    );
    assert_eq!(
        settings.categories, None,
        "a non-string tag rejects the whole array"
    );
    assert_eq!(settings.single_player, None, "no fixedAdapter declared");
    assert_eq!(
        settings.show_in_places, None,
        "no placesConfig.optOut declared"
    );
    assert_eq!(
        settings.thumbnail_hash, None,
        "a non-image navmapThumbnail must not be promoted into settings"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn settings_version_tracks_settings_writes() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x7777777777777777777777777777777777777777";
    let contents_dir = scratch_contents_dir("version");
    let auth = json!([{ "type": "SIGNER", "payload": owner }]);
    let parcels = ["0,0".to_string(), "0,1".to_string()];

    let version = |wc: WorldsComponent| async move {
        wc.get_world_settings("ver.dcl.eth")
            .await
            .unwrap()
            .expect("settings")
            .settings_version
    };

    wc.deploy_scene(
        "ver.dcl.eth",
        Some(owner),
        "bafyver1",
        owner,
        &auth,
        &deploy_entity("Version World", "bafymissing"),
        &parcels,
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("first deploy");
    assert_eq!(
        version(wc.clone()).await,
        0,
        "first deploy starts at the default"
    );

    wc.deploy_scene(
        "ver.dcl.eth",
        Some(owner),
        "bafyver2",
        owner,
        &auth,
        &deploy_entity("Version World", "bafymissing"),
        &parcels,
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("identical redeploy");
    assert_eq!(
        version(wc.clone()).await,
        0,
        "an unchanged republish must not bump"
    );

    wc.deploy_scene(
        "ver.dcl.eth",
        Some(owner),
        "bafyver3",
        owner,
        &auth,
        &deploy_entity("Version World Renamed", "bafymissing"),
        &parcels,
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("changed redeploy");
    assert_eq!(
        version(wc.clone()).await,
        1,
        "a settings-changing redeploy bumps"
    );

    let (after_put, _) = wc
        .update_world_settings(
            "ver.dcl.eth",
            owner,
            &catalyrst_worlds::ports::worlds::WorldSettingsUpdate {
                title: Some("Owner Title".into()),
                ..Default::default()
            },
        )
        .await
        .expect("settings update");
    assert_eq!(
        after_put.settings_version, 2,
        "PUT with a settings field bumps"
    );

    let (after_spawn, _) = wc
        .update_world_settings(
            "ver.dcl.eth",
            owner,
            &catalyrst_worlds::ports::worlds::WorldSettingsUpdate {
                spawn_coordinates: Some("0,1".into()),
                ..Default::default()
            },
        )
        .await
        .expect("spawn-only update");
    assert_eq!(
        after_spawn.settings_version, 2,
        "spawn coordinates are not a versioned settings column"
    );

    wc.store_access("ver.dcl.eth", &AccessSetting::Unrestricted)
        .await
        .expect("store access");
    assert_eq!(version(wc.clone()).await, 3, "an access change bumps");

    scratch.drop().await;
}

#[tokio::test]
async fn settings_spawn_validation_runs_inside_the_update_transaction() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x8888888888888888888888888888888888888888";
    let spawn_update = |spawn: &str| catalyrst_worlds::ports::worlds::WorldSettingsUpdate {
        spawn_coordinates: Some(spawn.into()),
        ..Default::default()
    };
    let bad_request = |err: ApiError| match err {
        ApiError::Common(catalyrst_types::ApiError::Http {
            status: 400,
            message,
        }) => message,
        other => panic!("expected BadRequest, got {other:?}"),
    };

    let err = wc
        .update_world_settings("spawnless.dcl.eth", owner, &spawn_update("0,0"))
        .await
        .expect_err("a world without scenes cannot take a spawn");
    assert!(
        bad_request(err).contains("has no deployed scenes"),
        "spawn on a sceneless world names the reason"
    );
    assert!(
        wc.get_world("spawnless.dcl.eth").await.unwrap().is_none(),
        "the row materialized for the lock rolls back with the failed validation"
    );

    let contents_dir = scratch_contents_dir("spawnlock");
    wc.deploy_scene(
        "spawnlock.dcl.eth",
        Some(owner),
        "bafyspawn",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("Spawn World", "bafymissing"),
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy");

    let err = wc
        .update_world_settings("spawnlock.dcl.eth", owner, &spawn_update("5,5"))
        .await
        .expect_err("a spawn outside the world shape is refused");
    assert!(
        bad_request(err).contains("must be within the world shape rectangle"),
        "out-of-shape spawn names the rectangle"
    );
    let settings = wc
        .get_world_settings("spawnlock.dcl.eth")
        .await
        .unwrap()
        .expect("settings");
    assert_eq!(
        settings.spawn_coordinates.as_deref(),
        Some("0,0"),
        "a refused spawn leaves the stored value untouched"
    );

    let (updated, old_spawn) = wc
        .update_world_settings("spawnlock.dcl.eth", owner, &spawn_update("0,1"))
        .await
        .expect("in-shape spawn");
    assert_eq!(updated.spawn_coordinates.as_deref(), Some("0,1"));
    assert_eq!(old_spawn.as_deref(), Some("0,0"));

    scratch.drop().await;
}

#[tokio::test]
async fn permission_parcels_lifecycle() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
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
            None,
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

    scratch.drop().await;
}

#[tokio::test]
async fn access_allow_list_modify() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());

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

    scratch.drop().await;
}

#[tokio::test]
async fn active_content_keys_names_every_referenced_blob() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x2222222222222222222222222222222222222222";
    let contents_dir = scratch_contents_dir("gc");

    wc.deploy_scene(
        "gc.dcl.eth",
        Some(owner),
        "bafyoldscene",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("Old", "bafyoldthumb"),
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("first deploy");

    wc.deploy_scene(
        "gc.dcl.eth",
        Some(owner),
        "bafynewscene",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("New", "bafynewthumb"),
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("second deploy");

    sqlx::query("UPDATE worlds SET thumbnail_hash = $1 WHERE name = $2")
        .bind("aabbccdd")
        .bind("gc.dcl.eth")
        .execute(&scratch.pool)
        .await
        .unwrap();

    let keys = catalyrst_worlds::handlers::gc::active_content_keys(&scratch.pool)
        .await
        .expect("active keys");

    for expected in [
        "bafynewscene",
        "bafynewscene.auth",
        "bafynewthumb",
        "aabbccdd",
    ] {
        assert!(keys.contains(expected), "missing active key {expected}");
    }
    for replaced in ["bafyoldscene", "bafyoldscene.auth", "bafyoldthumb"] {
        assert!(
            !keys.contains(replaced),
            "replaced scene still active: {replaced}"
        );
    }

    scratch.drop().await;
}

#[tokio::test]
async fn active_content_keys_fails_closed_on_an_unreadable_entity() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x3333333333333333333333333333333333333333";

    wc.deploy_scene(
        "broken.dcl.eth",
        Some(owner),
        "bafybroken",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("Broken", "bafythumb"),
        &["0,0".to_string()],
        1,
        &scratch_contents_dir("broken"),
        &catalyrst_worlds::ports::worlds::SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy");

    sqlx::query("UPDATE world_scenes SET entity = $1::jsonb WHERE entity_id = $2")
        .bind(r#"{"content": [{"file": "a.png"}]}"#)
        .bind("bafybroken")
        .execute(&scratch.pool)
        .await
        .unwrap();
    assert!(
        catalyrst_worlds::handlers::gc::active_content_keys(&scratch.pool)
            .await
            .is_err(),
        "a content entry without a hash must abort the whole active set"
    );

    sqlx::query("UPDATE world_scenes SET entity = 'null'::jsonb WHERE entity_id = $1")
        .bind("bafybroken")
        .execute(&scratch.pool)
        .await
        .unwrap();
    assert!(
        catalyrst_worlds::handlers::gc::active_content_keys(&scratch.pool)
            .await
            .is_err(),
        "a non-object entity must abort the whole active set"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn scoped_replacement_conflicts_on_unauthorized_overlap() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";
    let contents_dir = scratch_contents_dir("scoped");
    std::fs::write(contents_dir.join("bafythumb"), PNG_MAGIC).unwrap();

    wc.deploy_scene(
        "scoped.dcl.eth",
        Some(owner),
        "bafyA",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity_at("A", "bafythumb", "0,0", &["0,0", "0,1"]),
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy A");

    let conflict = wc
        .deploy_scene(
            "scoped.dcl.eth",
            Some(owner),
            "bafyB",
            owner,
            &json!([{ "type": "SIGNER", "payload": owner }]),
            &deploy_entity_at("B", "bafythumb", "0,0", &["0,0"]),
            &["0,0".to_string()],
            1,
            &contents_dir,
            &SceneReplacement::Scoped(vec![]),
        )
        .await;
    assert!(
        matches!(&conflict, Err(e) if e.is_conflict()),
        "unauthorized overlap must 409, got {conflict:?}"
    );
    let scenes = wc.get_scenes("scoped.dcl.eth").await.unwrap();
    assert_eq!(scenes.len(), 1);
    assert_eq!(scenes[0].entity_id, "bafyA");

    wc.deploy_scene(
        "scoped.dcl.eth",
        Some(owner),
        "bafyB",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity_at("B", "bafythumb", "0,0", &["0,0"]),
        &["0,0".to_string()],
        1,
        &contents_dir,
        &SceneReplacement::Scoped(vec!["bafyA".to_string()]),
    )
    .await
    .expect("deploy B replacing A");

    let scenes = wc.get_scenes("scoped.dcl.eth").await.unwrap();
    assert_eq!(scenes.len(), 1);
    assert_eq!(scenes[0].entity_id, "bafyB");
    assert_eq!(scenes[0].parcels, vec!["0,0".to_string()]);

    scratch.drop().await;
}

#[tokio::test]
async fn world_about_joins_world_row_and_scenes_newest_first() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";
    let contents_dir = scratch_contents_dir("about");
    std::fs::write(contents_dir.join("bafythumb"), PNG_MAGIC).unwrap();

    let none = wc.get_world_with_scenes("nothing.dcl.eth").await.unwrap();
    assert!(none.world.is_none() && none.scenes.is_empty());

    wc.create_basic_world_if_not_exists("empty.dcl.eth", owner)
        .await
        .unwrap();
    let empty = wc.get_world_with_scenes("empty.dcl.eth").await.unwrap();
    assert_eq!(
        empty.world.as_ref().map(|w| w.name.as_str()),
        Some("empty.dcl.eth")
    );
    assert!(empty.scenes.is_empty());

    wc.deploy_scene(
        "test.dcl.eth",
        Some(owner),
        "bafyentity",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity("My World", "bafythumb"),
        &["0,0".to_string(), "0,1".to_string()],
        123,
        &contents_dir,
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy_scene");
    wc.deploy_scene(
        "test.dcl.eth",
        Some(owner),
        "bafynewer",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &deploy_entity_at("Annex", "bafythumb", "5,5", &["5,5"]),
        &["5,5".to_string()],
        1,
        &contents_dir,
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy_scene 2");
    let about = wc.world_about("TEST.dcl.eth").await.unwrap();
    let world = about.world.as_ref().expect("world row");
    assert_eq!(world.owner.as_deref(), Some(owner));
    assert_eq!(world.skybox_time, Some(36000));
    let ids: Vec<&str> = about.scenes.iter().map(|s| s.entity_id.as_str()).collect();
    assert_eq!(ids, vec!["bafynewer", "bafyentity"]);

    // The memo drops on a local write.
    wc.undeploy_world("test.dcl.eth").await.unwrap();
    let after = wc.world_about("test.dcl.eth").await.unwrap();
    assert!(after.scenes.is_empty());

    scratch.drop().await;
}

#[tokio::test]
async fn single_statement_permission_routes_keep_their_row_shapes() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";
    let a = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let b = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let c = "0xcccccccccccccccccccccccccccccccccccccccc";

    // Unknown world: no row, no records.
    let (world, records) = wc
        .get_world_with_permission_records("one.dcl.eth")
        .await
        .unwrap();
    assert!(world.is_none());
    assert!(records.is_empty());

    // POST /permissions/{perm}: creates the world row and the listed grants.
    wc.replace_world_wide_permission("one.dcl.eth", owner, "deployment", &[a.into(), b.into()])
        .await
        .unwrap();
    // PUT .../{perm}/{address} on an existing row needs no ensure.
    let added = wc
        .grant_addresses_world_wide_permission("one.dcl.eth", "streaming", &[c.into()], None)
        .await
        .unwrap();
    assert_eq!(added, vec![c.to_string()]);
    // A repeat grant adds nothing.
    let added = wc
        .grant_addresses_world_wide_permission("one.dcl.eth", "streaming", &[c.into()], None)
        .await
        .unwrap();
    assert!(added.is_empty());

    let (world, records) = wc
        .get_world_with_permission_records("ONE.dcl.eth")
        .await
        .unwrap();
    assert_eq!(world.unwrap().owner.as_deref(), Some(owner));
    let full = wc
        .get_world_permission_records_full("one.dcl.eth")
        .await
        .unwrap();
    assert_eq!(records.len(), 3);
    for (agg, row) in records.iter().zip(full.iter()) {
        assert_eq!(agg.id, row.id);
        assert_eq!(agg.permission_type, row.permission_type);
        assert_eq!(agg.address, row.address);
        assert_eq!(agg.is_world_wide, row.is_world_wide);
        assert_eq!(agg.parcel_count, row.parcel_count);
    }

    // Replacing the list drops b, keeps a, adds c; a keeps its row (and id).
    let a_id = full
        .iter()
        .find(|r| r.address == a && r.permission_type == "deployment")
        .unwrap()
        .id;
    wc.replace_world_wide_permission("one.dcl.eth", owner, "deployment", &[a.into(), c.into()])
        .await
        .unwrap();
    let (_, records) = wc
        .get_world_with_permission_records("one.dcl.eth")
        .await
        .unwrap();
    let deployment: Vec<(i32, &str)> = records
        .iter()
        .filter(|r| r.permission_type == "deployment")
        .map(|r| (r.id, r.address.as_str()))
        .collect();
    assert_eq!(deployment.len(), 2);
    assert!(deployment.contains(&(a_id, a)));
    assert!(deployment.iter().any(|(_, addr)| *addr == c));
    assert!(!deployment.iter().any(|(_, addr)| *addr == b));

    // Parcel scoping: created flag, page + total in one statement, by-address removal.
    assert!(wc
        .add_parcels_to_permission(
            "one.dcl.eth",
            "deployment",
            b,
            &["00,00".into(), "2,3".into()]
        )
        .await
        .unwrap());
    assert!(!wc
        .add_parcels_to_permission(
            "one.dcl.eth",
            "deployment",
            b,
            &["2,3".into(), "5,5".into()]
        )
        .await
        .unwrap());
    let (total, parcels) = wc
        .get_parcels_for_permission_by_address("one.dcl.eth", "deployment", b, 2, 0, None)
        .await
        .unwrap()
        .expect("b holds a deployment permission");
    assert_eq!(total, 3);
    assert_eq!(parcels, vec!["0,0".to_string(), "2,3".to_string()]);
    let (total, parcels) = wc
        .get_parcels_for_permission_by_address("one.dcl.eth", "deployment", b, 10, 10, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((total, parcels), (3, Vec::new()));
    let (total, parcels) = wc
        .get_parcels_for_permission_by_address(
            "one.dcl.eth",
            "deployment",
            b,
            10,
            0,
            Some((5, 5, 2, 3)),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (total, parcels),
        (2, vec!["2,3".to_string(), "5,5".to_string()])
    );
    assert!(wc
        .get_parcels_for_permission_by_address("one.dcl.eth", "streaming", b, 10, 0, None)
        .await
        .unwrap()
        .is_none());

    // Coverage query: world-wide a covers anything; scoped b covers only its parcels.
    assert!(wc
        .has_deployment_permission_covering("one.dcl.eth", a, &["9,9".into()])
        .await
        .unwrap());
    assert!(wc
        .has_deployment_permission_covering("one.dcl.eth", b, &["0,0".into(), "5,5".into()])
        .await
        .unwrap());
    assert!(!wc
        .has_deployment_permission_covering("one.dcl.eth", b, &["0,0".into(), "9,9".into()])
        .await
        .unwrap());
    assert!(!wc
        .has_deployment_permission_covering("one.dcl.eth", owner, &["0,0".into()])
        .await
        .unwrap());

    let (atot, addrs) = wc
        .get_addresses_for_parcel_permission("one.dcl.eth", "deployment", &["0,0".into()], 1, 0)
        .await
        .unwrap();
    assert_eq!(atot, 3);
    assert_eq!(addrs, vec![a.to_string()]);
    let (atot, addrs) = wc
        .get_addresses_for_parcel_permission("one.dcl.eth", "deployment", &["0,0".into()], 5, 9)
        .await
        .unwrap();
    assert_eq!((atot, addrs), (3, Vec::new()));

    assert!(wc
        .remove_parcels_from_permission_by_address("one.dcl.eth", "deployment", b, &["0,0".into()])
        .await
        .unwrap());
    assert!(!wc
        .remove_parcels_from_permission_by_address("one.dcl.eth", "streaming", b, &["0,0".into()])
        .await
        .unwrap());
    assert!(!wc
        .remove_parcels_from_permission_by_address("one.dcl.eth", "streaming", b, &[])
        .await
        .unwrap());
    let (total, _) = wc
        .get_parcels_for_permission_by_address("one.dcl.eth", "deployment", b, 10, 0, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(total, 2);

    // A world without a deployed scene has no manifest; the CTE pages parcels in order.
    assert!(wc
        .get_world_manifest("one.dcl.eth")
        .await
        .unwrap()
        .is_none());

    scratch.drop().await;
}

#[tokio::test]
async fn allow_list_edits_are_guarded_updates() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";

    // No row yet: not an allow-list.
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xabc"))
            .await
            .unwrap(),
        AllowListEditOutcome::NotAllowList
    );

    wc.store_access_for_owner("al.dcl.eth", owner, &AccessSetting::Unrestricted)
        .await
        .unwrap();
    assert_eq!(
        wc.get_world_settings("al.dcl.eth")
            .await
            .unwrap()
            .unwrap()
            .settings_version,
        1
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xabc"))
            .await
            .unwrap(),
        AllowListEditOutcome::NotAllowList
    );

    wc.store_access_for_owner(
        "al.dcl.eth",
        owner,
        &AccessSetting::AllowList {
            wallets: vec!["0xABC".into()],
            communities: vec![],
        },
    )
    .await
    .unwrap();
    let version = || async {
        wc.get_world_settings("al.dcl.eth")
            .await
            .unwrap()
            .unwrap()
            .settings_version
    };
    assert_eq!(version().await, 2);

    // Case-insensitive presence keeps the stored spelling but still counts as a write.
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xabc"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xdef"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddCommunity("c-1"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddCommunity("c-2"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::RemoveWallet("0xabc"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::RemoveCommunity("c-1"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(version().await, 8);
    match wc.get_world("al.dcl.eth").await.unwrap().unwrap().access {
        AccessSetting::AllowList {
            wallets,
            communities,
        } => {
            assert_eq!(wallets, vec!["0xdef".to_string()]);
            assert_eq!(communities, vec!["c-2".to_string()]);
        }
        other => panic!("unexpected access {other:?}"),
    }

    // Caps: the wallet list refuses its 1001st entry, communities their 51st.
    let many: Vec<String> = (0..999).map(|i| format!("0x{i:040x}")).collect();
    wc.store_access_for_owner(
        "al.dcl.eth",
        owner,
        &AccessSetting::AllowList {
            wallets: many,
            communities: (0..50).map(|i| format!("c-{i}")).collect(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xlast"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xtoomany"))
            .await
            .unwrap(),
        AllowListEditOutcome::CapExceeded
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddWallet("0xlast"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddCommunity("c-new"))
            .await
            .unwrap(),
        AllowListEditOutcome::CapExceeded
    );
    assert_eq!(
        wc.modify_allow_list_access("al.dcl.eth", AllowListEdit::AddCommunity("c-7"))
            .await
            .unwrap(),
        AllowListEditOutcome::Applied
    );

    scratch.drop().await;
}

#[tokio::test]
async fn index_rows_project_only_what_the_summary_reads() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let owner = "0x1111111111111111111111111111111111111111";
    let contents_dir = scratch_contents_dir("index");
    std::fs::write(contents_dir.join("bafythumb"), PNG_MAGIC).unwrap();
    let mut entity = deploy_entity("Indexed", "bafythumb");
    entity["metadata"]["runtimeVersion"] = json!("7");
    entity["content"] = json!([
        { "file": "game.js", "hash": "bafygame" },
        { "file": "thumb.png", "hash": "bafythumb" }
    ]);
    wc.deploy_scene(
        "idx.dcl.eth",
        Some(owner),
        "bafyidx",
        owner,
        &json!([{ "type": "SIGNER", "payload": owner }]),
        &entity,
        &["0,0".to_string(), "0,1".to_string()],
        1,
        &contents_dir,
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .unwrap();

    let rows = wc.list_index_scenes(100, 0).await.unwrap();
    let (world, scene) = rows
        .iter()
        .find(|(w, _)| w == "idx.dcl.eth")
        .expect("indexed world");
    assert_eq!(world, "idx.dcl.eth");
    assert_eq!(scene.entity_id, "bafyidx");
    assert_eq!(scene.parcels, vec!["0,0".to_string(), "0,1".to_string()]);
    assert_eq!(scene.entity["timestamp"], json!(1000));
    assert_eq!(
        scene.entity["metadata"]["display"]["title"],
        json!("Indexed")
    );
    assert_eq!(scene.entity["metadata"]["runtimeVersion"], json!("7"));
    assert_eq!(
        scene.entity["content"],
        json!([{ "file": "thumb.png", "hash": "bafythumb" }])
    );
    assert!(scene.entity["metadata"].get("scene").is_none());

    let (page, total) = wc.admin_list_worlds_page(10, 0).await.unwrap();
    let row = page.iter().find(|w| w.name == "idx.dcl.eth").unwrap();
    assert_eq!(row.scene_count, 1);
    assert_eq!(total, page.len() as i64);
    let (empty, total_past_end) = wc.admin_list_worlds_page(10, 1000).await.unwrap();
    assert!(empty.is_empty());
    assert_eq!(total_past_end, total);

    scratch.drop().await;
}
