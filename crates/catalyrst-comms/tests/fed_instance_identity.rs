//! Regression tests for migration `0009_fed_instance_identity` and the
//! `FED_PEER_ID` / `epoch_author` federation-identity fix.
//!
//! This is security-relevant MLS identity code that previously had ZERO tests.
//! `mls_groups.epoch_author` decides which catalyst may advance a group's epoch;
//! before 0009 an unconfigured instance fell back to the literal `"local"`, so
//! every such instance claimed the SAME identity -- an epoch-author collision the
//! moment two of them shared a group. Migration 0009 mints one stable random id
//! per instance and lifts legacy `"local"` rows onto it, and `build_state`
//! resolves an unset `FED_PEER_ID` to that persisted id.
//!
//! These use `#[sqlx::test]`, which needs a live `DATABASE_URL` pointing at a
//! Postgres that grants `CREATE DATABASE` (it mints a throwaway DB per test).
//! They do NOT honour `ALLOW_SKIPPED_INTEGRATION`; run them against the scratch
//! cluster (`cargo test -p catalyrst-comms`).

use catalyrst_comms::build_state;
use catalyrst_comms::config::Config;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};

/// A `Config` with everything but `database_url` pinned to inert local values,
/// `fed_peer_id: None` (the case under test), and no dapps/places pools so
/// `build_state` performs no network or extra-DB work.
fn test_config(database_url: String) -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 5138,
        database_url,
        livekit_host: "livekit.local".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: false,
        private_messages_room_id: "private-messages".into(),
        places_api_url: "http://127.0.0.1:5134".into(),
        catalyst_url: "http://127.0.0.1:5141".into(),
        world_content_url: "http://127.0.0.1:5142".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        dapps_database_url: None,
        dapps_schema: "squid_marketplace".into(),
        places_database_url: None,
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
        fed_peer_id: None,
    }
}

/// (a) A freshly-migrated DB carries exactly one federation identity, and it is
/// the minted per-instance id -- never the collision-prone `"local"` literal.
#[sqlx::test(migrations = "./migrations")]
async fn fresh_db_seeds_exactly_one_fed_instance_identity(pool: PgPool) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fed_instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "migration 0009 must persist exactly one federation identity row"
    );

    let peer_id: String = sqlx::query_scalar("SELECT peer_id FROM fed_instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(
        peer_id, "local",
        "the minted id must not be the legacy collision literal"
    );
    assert!(
        peer_id.starts_with("instance-"),
        "unexpected minted id shape: {peer_id}"
    );
}

/// (b) With `FED_PEER_ID` unset, `build_state` resolves `fed_peer_id` to exactly
/// the id migration 0009 persisted in this database.
#[sqlx::test(migrations = "./migrations")]
async fn build_state_resolves_fed_peer_id_to_persisted_row(
    _pool_opts: PgPoolOptions,
    connect_opts: PgConnectOptions,
) {
    // `build_state` connects its own pool from `Config::database_url`, so hand it
    // the URL of this test's migrated database.
    let url = connect_opts.to_url_lossy().to_string();
    let cfg = test_config(url);
    assert!(
        cfg.fed_peer_id.is_none(),
        "test asserts the FED_PEER_ID-unset path"
    );

    let state = build_state(&cfg)
        .await
        .expect("build_state should succeed against a migrated DB");

    let persisted: String = sqlx::query_scalar("SELECT peer_id FROM fed_instance_identity")
        .fetch_one(&state.pool)
        .await
        .unwrap();

    assert_eq!(
        state.fed_peer_id, persisted,
        "unset FED_PEER_ID must resolve to the DB-persisted per-instance id"
    );
    assert_ne!(
        state.fed_peer_id, "local",
        "build_state must never fall back to the shared 'local' literal"
    );
}

/// (c) A pre-0009 `mls_groups` row whose `epoch_author` is the literal `"local"`
/// is rewritten onto the minted id by the backfill. Driven on a bare DB (no
/// `migrations` arg) so the legacy row can exist *before* 0009 runs -- the whole
/// point of the backfill.
#[sqlx::test]
async fn backfill_rewrites_legacy_local_epoch_author(pool: PgPool) {
    // 0004 creates `mls_groups`; it has no dependency on the intervening
    // migrations, and none of 0005..0008 touch `epoch_author`.
    sqlx::raw_sql(include_str!("../migrations/0004_mls_messaging.sql"))
        .execute(&pool)
        .await
        .expect("apply migration 0004");

    // A group minted under the old default -- epoch_author == "local".
    sqlx::query(
        "INSERT INTO mls_groups (group_id, creator, group_kind, epoch_author, ciphersuite) \
         VALUES ($1, $2, 'channel', 'local', 1)",
    )
    .bind("abcd")
    .bind("0x1111111111111111111111111111111111111111")
    .execute(&pool)
    .await
    .expect("seed a legacy 'local' group");

    sqlx::raw_sql(include_str!("../migrations/0009_fed_instance_identity.sql"))
        .execute(&pool)
        .await
        .expect("apply migration 0009");

    let minted: String = sqlx::query_scalar("SELECT peer_id FROM fed_instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    let rewritten: String =
        sqlx::query_scalar("SELECT epoch_author FROM mls_groups WHERE group_id = 'abcd'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_ne!(
        rewritten, "local",
        "the backfill must not leave 'local' in place"
    );
    assert_eq!(
        rewritten, minted,
        "the legacy 'local' epoch_author must be lifted onto the minted per-instance id"
    );
}

/// (d) Re-running migration 0009 is a no-op: the `IF NOT EXISTS` /
/// `ON CONFLICT DO NOTHING` guards keep the single row and its minted id intact,
/// and the `WHERE epoch_author = 'local'` backfill matches nothing on a second
/// pass.
#[sqlx::test(migrations = "./migrations")]
async fn rerunning_migration_0009_is_idempotent(pool: PgPool) {
    let before: String = sqlx::query_scalar("SELECT peer_id FROM fed_instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();

    sqlx::raw_sql(include_str!("../migrations/0009_fed_instance_identity.sql"))
        .execute(&pool)
        .await
        .expect("re-applying migration 0009 must not error");

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fed_instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    let after: String = sqlx::query_scalar("SELECT peer_id FROM fed_instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        count, 1,
        "re-running 0009 must not add a second identity row"
    );
    assert_eq!(
        before, after,
        "re-running 0009 must not change the minted id"
    );
}
