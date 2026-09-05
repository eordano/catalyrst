use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_db::advisory_locked_count;
use sqlx::PgPool;

const COUNT_SQL: &str = "SELECT COUNT(*) FROM quota_probe WHERE address = $1";
const MAX_PER_KEY: i64 = 1;

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_DB_TEST_PG", "cg_db_advisory_quota").await?;
    sqlx::query("CREATE TABLE quota_probe (address text NOT NULL, created_at timestamptz NOT NULL DEFAULT now())")
        .execute(&scratch.pool)
        .await
        .expect("create quota_probe");
    Some(scratch)
}

async fn reserve(pool: &PgPool, address: &str) -> Result<(), ()> {
    let mut tx = pool.begin().await.expect("begin");

    let count = advisory_locked_count(&mut tx, address, COUNT_SQL)
        .await
        .expect("advisory_locked_count");

    if count >= MAX_PER_KEY {
        tx.rollback().await.expect("rollback");
        return Err(());
    }

    sqlx::query("INSERT INTO quota_probe (address) VALUES ($1)")
        .bind(address)
        .execute(&mut *tx)
        .await
        .expect("insert");
    tx.commit().await.expect("commit");
    Ok(())
}

#[tokio::test]
async fn counts_only_the_given_key() {
    let Some(scratch) = setup().await else {
        return;
    };

    reserve(&scratch.pool, "alice")
        .await
        .expect("first alice reserve");
    reserve(&scratch.pool, "bob")
        .await
        .expect("bob is a separate key");

    let mut tx = scratch.pool.begin().await.expect("begin");
    let alice_count = advisory_locked_count(&mut tx, "alice", COUNT_SQL)
        .await
        .expect("count alice");
    assert_eq!(alice_count, 1);
    let carol_count = advisory_locked_count(&mut tx, "carol", COUNT_SQL)
        .await
        .expect("count carol");
    assert_eq!(carol_count, 0, "an untouched key must count zero rows");
    tx.rollback().await.expect("rollback");

    scratch.drop().await;
}

#[tokio::test]
async fn a_rolled_back_reservation_leaves_no_row() {
    let Some(scratch) = setup().await else {
        return;
    };

    reserve(&scratch.pool, "dana")
        .await
        .expect("dana under quota");
    let second = reserve(&scratch.pool, "dana").await;
    assert!(second.is_err(), "the second reservation must hit quota");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quota_probe WHERE address = $1")
        .bind("dana")
        .fetch_one(&scratch.pool)
        .await
        .expect("row count");
    assert_eq!(rows, 1, "the rejected attempt's rollback must not persist");

    scratch.drop().await;
}

#[tokio::test]
async fn concurrent_reservations_on_the_same_key_yield_exactly_one_winner() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move { reserve(&pool, "erin").await }));
    }

    let mut wins = 0;
    let mut over_quota = 0;
    for h in handles {
        match h.await.expect("task") {
            Ok(()) => wins += 1,
            Err(()) => over_quota += 1,
        }
    }
    assert_eq!(
        wins, 1,
        "the advisory lock must serialize the race to one winner"
    );
    assert_eq!(over_quota, 7);

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quota_probe WHERE address = $1")
        .bind("erin")
        .fetch_one(&pool)
        .await
        .expect("row count");
    assert_eq!(rows, 1);

    scratch.drop().await;
}
