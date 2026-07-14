//! Component-level cover for the scene-identity half of upstream #521:
//!
//! * a stored scene's downstream identity (`effective_base_parcel`) is never the deployer's
//!   declared base unless that base is inside the scene's own footprint, and
//! * `deploy_scene` / `undeploy_scene` delete only the scene identities the caller was
//!   authorized for, rolling back when an unauthorized scene survives.

use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_worlds::http::ApiError;
use catalyrst_worlds::ports::worlds::{SceneReplacement, WorldsComponent};
use serde_json::json;

const OWNER: &str = "0x1111111111111111111111111111111111111111";

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLDS_TEST_PG", "cg_scene_identity").await?;
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

fn scene_entity(base: &str, parcels: &[&str]) -> serde_json::Value {
    json!({
        "type": "scene",
        "timestamp": 1000,
        "pointers": parcels,
        "metadata": { "scene": { "base": base, "parcels": parcels } }
    })
}

fn owned(parcels: &[&str]) -> Vec<String> {
    parcels.iter().map(|p| p.to_string()).collect()
}

async fn deploy(
    wc: &WorldsComponent,
    world: &str,
    entity_id: &str,
    base: &str,
    parcels: &[&str],
    replacement: &SceneReplacement,
) -> Result<(), ApiError> {
    wc.deploy_scene(
        world,
        Some(OWNER),
        entity_id,
        OWNER,
        &json!([{ "type": "SIGNER", "payload": OWNER }]),
        &scene_entity(base, parcels),
        &owned(parcels),
        1,
        &std::env::temp_dir(),
        replacement,
    )
    .await
}

async fn entity_ids(wc: &WorldsComponent, world: &str) -> Vec<String> {
    let mut ids: Vec<String> = wc
        .list_scenes(world)
        .await
        .expect("list_scenes")
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_base_outside_the_scene_footprint_never_becomes_the_scene_identity() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let world = "identity.dcl.eth";

    // The scene claims a base parcel it does not occupy. Trusting it would send the comms
    // gatekeeper's scene-ban probe to `/parcels/100,100/...`, where nobody has bans.
    deploy(
        &wc,
        world,
        "bafyliar",
        "100,100",
        &["0,0", "0,1"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy liar");

    assert_eq!(
        wc.get_scene_base_parcel(world, "bafyliar").await.unwrap(),
        Some("0,0".to_string()),
        "a base outside the footprint must fall back to the scene's own first parcel"
    );

    deploy(
        &wc,
        world,
        "bafyhonest",
        "1,1",
        &["1,0", "1,1"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("deploy honest");
    assert_eq!(
        wc.get_scene_base_parcel(world, "bafyhonest").await.unwrap(),
        Some("1,1".to_string()),
        "a genuine base inside the footprint is still honoured"
    );

    let bases: Vec<(String, Option<String>)> = wc
        .list_scenes(world)
        .await
        .unwrap()
        .into_iter()
        .map(|(id, _, base)| (id, base))
        .collect();
    assert!(bases.contains(&("bafyliar".to_string(), Some("0,0".to_string()))));
    assert!(bases.contains(&("bafyhonest".to_string(), Some("1,1".to_string()))));

    scratch.drop().await;
}

#[tokio::test]
async fn a_scoped_deploy_replaces_only_the_authorized_identities() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let world = "scoped.dcl.eth";

    deploy(
        &wc,
        world,
        "bafyexisting",
        "0,0",
        &["0,0", "0,1"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("existing scene");
    deploy(
        &wc,
        world,
        "bafyneighbour",
        "5,5",
        &["5,5"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("neighbour scene");

    let overlapping: Vec<(String, Vec<String>)> = wc
        .scenes_overlapping_parcels(world, &owned(&["0,0"]))
        .await
        .expect("overlap")
        .into_iter()
        .map(|s| (s.entity_id, s.parcels))
        .collect();
    assert_eq!(
        overlapping,
        vec![("bafyexisting".to_string(), owned(&["0,0", "0,1"]))],
        "the overlap query must report the replaced scene's WHOLE footprint"
    );

    deploy(
        &wc,
        world,
        "bafynew",
        "0,0",
        &["0,0"],
        &SceneReplacement::Scoped(vec!["bafyexisting".to_string()]),
    )
    .await
    .expect("scoped deploy");

    assert_eq!(
        entity_ids(&wc, world).await,
        vec!["bafyneighbour".to_string(), "bafynew".to_string()],
        "only the authorized identity may be replaced"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_scoped_deploy_rolls_back_when_an_unauthorized_scene_survives() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let world = "conflict.dcl.eth";

    deploy(
        &wc,
        world,
        "bafyexisting",
        "0,0",
        &["0,0", "0,1"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("existing scene");

    // The authorization snapshot said nothing overlapped; by the time the transaction runs
    // something does. The deployment must not silently land on top of it.
    let err = deploy(
        &wc,
        world,
        "bafyracer",
        "0,0",
        &["0,0"],
        &SceneReplacement::Scoped(Vec::new()),
    )
    .await
    .expect_err("stale authorization must not deploy");
    assert!(
        matches!(err, ApiError::Conflict(ref m) if m.contains(world)),
        "expected a replacement conflict naming {world}, got {err:?}"
    );

    assert_eq!(
        entity_ids(&wc, world).await,
        vec!["bafyexisting".to_string()],
        "the whole transaction must roll back -- no partial delete, no new scene"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn undeploy_scene_removes_only_the_authorized_identities() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let wc = WorldsComponent::new(scratch.pool.clone());
    let world = "undeploy.dcl.eth";

    deploy(
        &wc,
        world,
        "bafywide",
        "0,0",
        &["0,0", "0,1"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("wide scene");
    deploy(
        &wc,
        world,
        "bafyother",
        "5,5",
        &["5,5"],
        &SceneReplacement::UnrestrictedOwner,
    )
    .await
    .expect("other scene");

    assert_eq!(
        wc.undeploy_scene(world, "0,0", Some(&[])).await.unwrap(),
        0,
        "an empty authorization set deletes nothing"
    );
    assert_eq!(entity_ids(&wc, world).await.len(), 2);

    assert_eq!(
        wc.undeploy_scene(world, "0,0", Some(&["bafywide".to_string()]))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        entity_ids(&wc, world).await,
        vec!["bafyother".to_string()],
        "the unauthorized neighbour must survive"
    );

    // The owner path passes no allow-list at all.
    assert_eq!(wc.undeploy_scene(world, "5,5", None).await.unwrap(), 1);
    assert!(entity_ids(&wc, world).await.is_empty());

    scratch.drop().await;
}
