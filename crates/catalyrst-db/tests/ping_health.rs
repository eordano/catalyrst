use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_db::ping_health;

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create("CATALYRST_DB_TEST_PG", "cg_db_ping_health").await
}

#[tokio::test]
async fn ping_health_succeeds_against_a_live_pool() {
    let Some(scratch) = setup().await else {
        return;
    };

    ping_health(&scratch.pool)
        .await
        .expect("SELECT 1 against a live pool must succeed");

    scratch.drop().await;
}

#[tokio::test]
async fn ping_health_fails_once_the_pool_is_closed() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    pool.close().await;

    let err = ping_health(&pool)
        .await
        .expect_err("a closed pool must not answer SELECT 1");
    assert!(
        matches!(err, sqlx::Error::PoolClosed),
        "expected PoolClosed, got {err:?}"
    );

    scratch.drop().await;
}
