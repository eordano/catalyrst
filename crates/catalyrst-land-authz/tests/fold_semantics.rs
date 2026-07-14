use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_land_authz::events::{
    AuthzEvent, KIND_APPROVAL, KIND_APPROVED_FOR_ALL, KIND_TRANSFER, KIND_UPDATE_MANAGER,
    KIND_UPDATE_OPERATOR,
};
use catalyrst_land_authz::indexer::{fold_in, insert_events_in};

const LAND: &str = "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d";
const ALICE: &str = "0xaaaa000000000000000000000000000000000001";
const BOB: &str = "0xbbbb000000000000000000000000000000000002";
const OWNER: &str = "0x1111000000000000000000000000000000000003";

const DDL: &str = "
CREATE TABLE authz_event (
    block_number BIGINT NOT NULL, log_index INTEGER NOT NULL, block_time BIGINT NOT NULL,
    token_address TEXT NOT NULL, kind TEXT NOT NULL, token_id NUMERIC,
    account TEXT, operator TEXT, approved BOOLEAN,
    PRIMARY KEY (block_number, log_index));
CREATE TABLE token_right (
    token_address TEXT NOT NULL, token_id NUMERIC NOT NULL, x INTEGER, y INTEGER,
    operator TEXT, update_operator TEXT, updated_block BIGINT NOT NULL, updated_log INTEGER NOT NULL,
    PRIMARY KEY (token_address, token_id));
CREATE TABLE account_right (
    token_address TEXT NOT NULL, account TEXT NOT NULL, operator TEXT NOT NULL, kind TEXT NOT NULL,
    is_approved BOOLEAN NOT NULL, updated_block BIGINT NOT NULL, updated_log INTEGER NOT NULL,
    PRIMARY KEY (token_address, account, operator, kind));
CREATE FUNCTION decode_coord(half NUMERIC) RETURNS INTEGER LANGUAGE sql IMMUTABLE AS $$
  SELECT (CASE WHEN half >= 170141183460469231731687303715884105728::numeric
               THEN half - 340282366920938463463374607431768211456::numeric ELSE half END)::INTEGER $$;
CREATE FUNCTION token_x(token_id NUMERIC) RETURNS INTEGER LANGUAGE sql IMMUTABLE AS $$
  SELECT decode_coord(div(token_id, 340282366920938463463374607431768211456::numeric)) $$;
CREATE FUNCTION token_y(token_id NUMERIC) RETURNS INTEGER LANGUAGE sql IMMUTABLE AS $$
  SELECT decode_coord(mod(token_id, 340282366920938463463374607431768211456::numeric)) $$;
";

fn ev(
    block: i64,
    index: i32,
    kind: &'static str,
    token: Option<&str>,
    account: Option<&str>,
    operator: Option<&str>,
    approved: Option<bool>,
) -> AuthzEvent {
    AuthzEvent {
        block_number: block,
        log_index: index,
        block_time: block * 12,
        token_address: LAND.to_string(),
        kind,
        token_id: token.map(|t| t.to_string()),
        account: account.map(|a| a.to_string()),
        operator: operator.map(|o| o.to_string()),
        approved,
    }
}

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_LAND_AUTHZ_TEST_PG", "laz_fold").await?;
    scratch.apply_sql(DDL).await;
    Some(scratch)
}

async fn token_leg(scratch: &ScratchSchema, token: &str, column: &str) -> Option<String> {
    let sql = format!("SELECT {column} FROM token_right WHERE token_id = $1::numeric");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(token)
        .fetch_optional(&scratch.pool)
        .await
        .unwrap()
        .flatten()
}

async fn account_leg(scratch: &ScratchSchema, account: &str, kind: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT operator FROM account_right WHERE account = $1 AND kind = $2 AND is_approved ORDER BY operator",
    )
    .bind(account)
    .bind(kind)
    .fetch_all(&scratch.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_granted_then_revoked_update_operator_ends_revoked() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                10,
                0,
                KIND_UPDATE_OPERATOR,
                Some("1"),
                None,
                Some(ALICE),
                None,
            ),
            ev(
                20,
                0,
                KIND_UPDATE_OPERATOR,
                Some("1"),
                None,
                Some("0x0000000000000000000000000000000000000000"),
                None,
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert_eq!(token_leg(&scratch, "1", "update_operator").await, None);
    scratch.drop().await;
}

#[tokio::test]
async fn the_later_grant_wins_regardless_of_insertion_order() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                99,
                0,
                KIND_UPDATE_OPERATOR,
                Some("7"),
                None,
                Some(BOB),
                None,
            ),
            ev(
                5,
                0,
                KIND_UPDATE_OPERATOR,
                Some("7"),
                None,
                Some(ALICE),
                None,
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert_eq!(
        token_leg(&scratch, "7", "update_operator").await.as_deref(),
        Some(BOB)
    );
    scratch.drop().await;
}

#[tokio::test]
async fn log_index_breaks_ties_inside_one_block() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                42,
                3,
                KIND_UPDATE_OPERATOR,
                Some("9"),
                None,
                Some(BOB),
                None,
            ),
            ev(
                42,
                1,
                KIND_UPDATE_OPERATOR,
                Some("9"),
                None,
                Some(ALICE),
                None,
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert_eq!(
        token_leg(&scratch, "9", "update_operator").await.as_deref(),
        Some(BOB)
    );
    scratch.drop().await;
}

#[tokio::test]
async fn a_transfer_clears_both_per_token_legs() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                10,
                0,
                KIND_UPDATE_OPERATOR,
                Some("3"),
                None,
                Some(ALICE),
                None,
            ),
            ev(
                10,
                1,
                KIND_APPROVAL,
                Some("3"),
                Some(OWNER),
                Some(BOB),
                None,
            ),
            ev(30, 0, KIND_TRANSFER, Some("3"), Some(BOB), None, None),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert_eq!(token_leg(&scratch, "3", "update_operator").await, None);
    assert_eq!(token_leg(&scratch, "3", "operator").await, None);
    scratch.drop().await;
}

#[tokio::test]
async fn a_grant_after_a_transfer_survives_it() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(30, 0, KIND_TRANSFER, Some("4"), Some(BOB), None, None),
            ev(
                31,
                0,
                KIND_UPDATE_OPERATOR,
                Some("4"),
                None,
                Some(ALICE),
                None,
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert_eq!(
        token_leg(&scratch, "4", "update_operator").await.as_deref(),
        Some(ALICE)
    );
    scratch.drop().await;
}

#[tokio::test]
async fn approval_for_all_is_account_wide_and_revocable() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                10,
                0,
                KIND_APPROVED_FOR_ALL,
                None,
                Some(OWNER),
                Some(ALICE),
                Some(true),
            ),
            ev(
                11,
                0,
                KIND_APPROVED_FOR_ALL,
                None,
                Some(OWNER),
                Some(BOB),
                Some(true),
            ),
            ev(
                12,
                0,
                KIND_APPROVED_FOR_ALL,
                None,
                Some(OWNER),
                Some(ALICE),
                Some(false),
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert_eq!(
        account_leg(&scratch, OWNER, "approved_for_all").await,
        vec![BOB.to_string()]
    );
    scratch.drop().await;
}

#[tokio::test]
async fn update_manager_and_approval_for_all_do_not_bleed_into_each_other() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                10,
                0,
                KIND_UPDATE_MANAGER,
                None,
                Some(OWNER),
                Some(ALICE),
                Some(true),
            ),
            ev(
                11,
                0,
                KIND_APPROVED_FOR_ALL,
                None,
                Some(OWNER),
                Some(BOB),
                Some(true),
            ),
            ev(
                12,
                0,
                KIND_UPDATE_MANAGER,
                None,
                Some(OWNER),
                Some(ALICE),
                Some(false),
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert!(account_leg(&scratch, OWNER, "update_manager")
        .await
        .is_empty());
    assert_eq!(
        account_leg(&scratch, OWNER, "approved_for_all").await,
        vec![BOB.to_string()]
    );
    scratch.drop().await;
}

#[tokio::test]
async fn a_token_grant_never_becomes_an_account_grant() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[ev(
            10,
            0,
            KIND_UPDATE_OPERATOR,
            Some("5"),
            None,
            Some(ALICE),
            None,
        )],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    assert!(account_leg(&scratch, ALICE, "update_manager")
        .await
        .is_empty());
    assert!(account_leg(&scratch, ALICE, "approved_for_all")
        .await
        .is_empty());
    scratch.drop().await;
}

#[tokio::test]
async fn folding_twice_is_idempotent() {
    let Some(scratch) = setup().await else {
        return;
    };
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[
            ev(
                10,
                0,
                KIND_UPDATE_OPERATOR,
                Some("6"),
                None,
                Some(ALICE),
                None,
            ),
            ev(
                11,
                0,
                KIND_APPROVED_FOR_ALL,
                None,
                Some(OWNER),
                Some(BOB),
                Some(true),
            ),
        ],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    let tokens: i64 = sqlx::query_scalar("SELECT count(*) FROM token_right")
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
    assert_eq!(tokens, 1);
    assert_eq!(
        token_leg(&scratch, "6", "update_operator").await.as_deref(),
        Some(ALICE)
    );
    scratch.drop().await;
}

#[tokio::test]
async fn coordinates_come_back_out_of_the_token_id() {
    let Some(scratch) = setup().await else {
        return;
    };
    let token_37_55 = "12590447576074723148144860474975423823927";
    insert_events_in(
        &scratch.pool,
        &scratch.schema,
        &[ev(
            10,
            0,
            KIND_UPDATE_OPERATOR,
            Some(token_37_55),
            None,
            Some(ALICE),
            None,
        )],
    )
    .await
    .unwrap();
    fold_in(&scratch.pool, &scratch.schema).await.unwrap();
    let xy: (Option<i32>, Option<i32>) =
        sqlx::query_as("SELECT x, y FROM token_right WHERE token_id = $1::numeric")
            .bind(token_37_55)
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert_eq!(xy, (Some(37), Some(55)));
    scratch.drop().await;
}
