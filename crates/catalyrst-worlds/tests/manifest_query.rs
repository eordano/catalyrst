use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_worlds::ports::worlds::WorldsComponent;

#[tokio::test]
async fn manifest_keeps_total_order_limit_and_empty_worlds_in_one_query() {
    let Some(db) = ScratchSchema::create("CATALYRST_TEST_PG", "manifest_batch").await else {
        return;
    };
    db.apply_sql(include_str!("../migrations/0001_init.sql"))
        .await;
    sqlx::query(
        "INSERT INTO worlds(name,spawn_coordinates) VALUES ('Mixed.eth','-2,0'),('empty.eth',NULL)",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let parcels: Vec<_> = (-3..510).rev().map(|i| format!("{i},0")).collect();
    for id in ["a", "b"] {
        sqlx::query("INSERT INTO world_scenes(world_name,entity_id,deployment_auth_chain,entity,deployer,parcels,size) VALUES ('Mixed.eth',$1,'[]','{}','owner',$2,0)")
            .bind(id).bind(&parcels).execute(&db.pool).await.unwrap();
    }
    let worlds = WorldsComponent::new(db.pool.clone());
    let capture = catalyrst_testgate::sql_capture::sql_capture();
    let manifest = worlds
        .get_world_manifest("MIXED.ETH")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(capture.count(), 1);
    assert_eq!(manifest.total, 513);
    assert_eq!(
        manifest.parcels,
        (-3..497).map(|i| format!("{i},0")).collect::<Vec<_>>()
    );
    assert_eq!(manifest.spawn_coordinates.as_deref(), Some("-2,0"));
    for name in ["empty.eth", "missing.eth"] {
        capture.reset();
        assert!(worlds.get_world_manifest(name).await.unwrap().is_none());
        assert_eq!(capture.count(), 1);
    }
    sqlx::query("UPDATE worlds SET spawn_coordinates=NULL")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(worlds
        .get_world_manifest("Mixed.eth")
        .await
        .unwrap()
        .unwrap()
        .spawn_coordinates
        .is_none());
    db.drop().await;
}
