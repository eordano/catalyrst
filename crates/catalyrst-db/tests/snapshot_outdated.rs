use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_db::snapshots_repository::{snapshot_is_outdated, SnapshotMetadata, TimeRange};

#[tokio::test]
async fn outdated_probe_preserves_inclusive_range_and_active_late_arrivals() {
    let Some(db) = ScratchSchema::create("CATALYRST_DB_TEST_PG", "snapshot_outdated").await else {
        return;
    };
    sqlx::query("CREATE TABLE deployments (entity_timestamp timestamptz, local_timestamp timestamptz, deleter_deployment bigint)")
        .execute(&db.pool).await.unwrap();
    let snapshot = SnapshotMetadata {
        hash: None,
        time_range: TimeRange::new(1000.0, 2000.0),
        replaced_snapshot_hashes: vec![],
        number_of_entities: 0,
        generation_timestamp: 3000.0,
    };
    for (entity_ms, local_ms, deleted, expected) in [
        (999.0, 3001.0, None, false),
        (2001.0, 3001.0, None, false),
        (1500.0, 3000.0, None, false),
        (1500.0, 3001.0, Some(1_i64), false),
        (1000.0, 3001.0, None, true),
        (2000.0, 3001.0, None, true),
    ] {
        sqlx::query("TRUNCATE deployments")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO deployments VALUES (to_timestamp($1 / 1000.0), to_timestamp($2 / 1000.0), $3)")
            .bind(entity_ms).bind(local_ms).bind(deleted).execute(&db.pool).await.unwrap();
        assert_eq!(
            snapshot_is_outdated(&db.pool, &snapshot).await.unwrap(),
            expected
        );
    }
    sqlx::query("INSERT INTO deployments SELECT to_timestamp(1.5), to_timestamp(4), NULL FROM generate_series(1, 10000)")
        .execute(&db.pool).await.unwrap();
    assert!(snapshot_is_outdated(&db.pool, &snapshot).await.unwrap());
    db.drop().await;
}
