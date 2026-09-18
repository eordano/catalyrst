use catalyrst_economy::ports::claims::{
    claim_escrow_action, claim_name_purchase, claim_name_transfer, Claim,
};
use sqlx::PgPool;
use uuid::Uuid;

mod support;

const ESCROW: &str = "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4";
const COLLECTION: &str = "0x214ffc0f0103735728dc66b61a22e4f163e275ae";
const BUYER: &str = "0x1111111111111111111111111111111111111111";

async fn set_state(pool: &PgPool, table: &str, key: &str, status: &str, tx_hash: Option<&str>) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {table} SET status = $2, tx_hash = $3, updated_at = NOW() - INTERVAL '1 hour' \
         WHERE idempotency_key = $1"
    )))
    .bind(key)
    .bind(status)
    .bind(tx_hash)
    .execute(pool)
    .await
    .expect("seed state");
}

async fn stored(pool: &PgPool, table: &str, key: &str) -> (String, bool) {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT status, updated_at > NOW() - INTERVAL '1 minute' \
         FROM {table} WHERE idempotency_key = $1"
    )))
    .bind(key)
    .fetch_one(pool)
    .await
    .expect("stored row")
}

/// Every keyed broadcast claims its key the same way, so the three statements are
/// driven through one script: fresh claim, in-flight replay, settled replay, error
/// re-arm, and a second re-arm attempt that must find the row already 'pending'.
async fn exercise<F, Fut>(pool: &PgPool, table: &str, claim: F)
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Claim>,
{
    let key = Uuid::new_v4().to_string();

    let fresh = claim(key.clone()).await;
    assert_eq!(
        fresh,
        Claim {
            claimed: true,
            rearmed: false,
            status: None,
            tx_hash: None
        },
        "{table}: a fresh key inserts the pending row"
    );

    let replay = claim(key.clone()).await;
    assert!(!replay.claimed && !replay.rearmed, "{table}: {replay:?}");
    assert_eq!(replay.status(), "pending");
    assert_eq!(replay.tx_hash, None);

    set_state(pool, table, &key, "sent", Some("0xabc")).await;
    let settled = claim(key.clone()).await;
    assert_eq!(
        settled,
        Claim {
            claimed: false,
            rearmed: false,
            status: Some("sent".into()),
            tx_hash: Some("0xabc".into())
        },
        "{table}: a settled row replays its hash"
    );
    let (status, touched) = stored(pool, table, &key).await;
    assert_eq!(status, "sent");
    assert!(
        !touched,
        "{table}: a settled row is not rewritten by a replay"
    );

    set_state(pool, table, &key, "error", None).await;
    let rearmed = claim(key.clone()).await;
    assert_eq!(
        rearmed,
        Claim {
            claimed: false,
            rearmed: true,
            status: Some("error".into()),
            tx_hash: None
        },
        "{table}: an errored row is re-armed and reports its prior status"
    );
    let (status, touched) = stored(pool, table, &key).await;
    assert_eq!(status, "pending");
    assert!(touched, "{table}: the re-arm bumps updated_at");

    let again = claim(key.clone()).await;
    assert!(!again.claimed && !again.rearmed, "{table}: {again:?}");
    assert_eq!(again.status(), "pending");
}

#[tokio::test]
async fn escrow_action_claims_cover_every_replay_state() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    exercise(&pool, "escrow_actions", |key| {
        let pool = pool.clone();
        async move {
            claim_escrow_action(
                &pool,
                &key,
                "release",
                COLLECTION,
                "7",
                Some(BUYER),
                ESCROW,
                137,
            )
            .await
            .expect("claim")
        }
    })
    .await;
    scratch.cleanup().await;
}

#[tokio::test]
async fn name_purchase_claims_cover_every_replay_state() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    exercise(&pool, "broker_purchases", |key| {
        let pool = pool.clone();
        async move {
            claim_name_purchase(
                &pool,
                &key,
                ESCROW,
                Some("alice"),
                None,
                BUYER,
                BUYER,
                "100000000000000000000",
                1,
                "name-mint",
            )
            .await
            .expect("claim")
        }
    })
    .await;
    scratch.cleanup().await;
}

#[tokio::test]
async fn name_transfer_claims_cover_every_replay_state() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    exercise(&pool, "name_transfers", |key| {
        let pool = pool.clone();
        async move {
            claim_name_transfer(&pool, &key, ESCROW, "7", BUYER, COLLECTION, 1)
                .await
                .expect("claim")
        }
    })
    .await;
    scratch.cleanup().await;
}
