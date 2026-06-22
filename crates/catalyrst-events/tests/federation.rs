//! DB-backed federation smoke test for the events moderator write path.
//!
//! Skips gracefully when no Postgres is reachable (CI without a DB). Set
//! `CATALYRST_EVENTS_TEST_PG` to a base connection string to run; each test
//! isolates itself in a throwaway schema dropped on completion.

use std::time::Duration;

use catalyrst_events::fed::apply;
use catalyrst_events::fed::authority::{is_moderator, require_moderator};
use catalyrst_events::fed::messages::{ProfileSettingsUpdate, ScheduleUpsert};
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{Signed, TypedMessage};
use ethers_signers::{LocalWallet, Signer};
use rand::RngCore;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_EVENTS_TEST_PG").ok()
}

fn unique_schema() -> String {
    let mut b = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut b);
    format!("test_evt_{}", hex::encode(b))
}

async fn setup() -> Option<(PgPool, String, String)> {
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
    apply_migration(&pool, include_str!("../migrations/0001_federation.sql")).await;
    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned: String = sql
        .lines()
        .filter(|l| !l.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    for stmt in cleaned.split(';') {
        if stmt.trim().is_empty() {
            continue;
        }
        sqlx::query(sqlx::AssertSqlSafe(stmt.to_owned()))
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("migration stmt failed: {e}\n{stmt}"));
    }
}

async fn cleanup(admin_url: &str, schema: &str) {
    if let Ok(admin) = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url)
        .await
    {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {} CASCADE", schema)))
            .execute(&admin)
            .await;
    }
}

fn mk_wallet(seed: u8) -> LocalWallet {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    LocalWallet::from_bytes(&key).expect("wallet from bytes")
}

fn addr(w: &LocalWallet) -> String {
    format!("{:#x}", w.address())
}

fn rand_nonce() -> [u8; 16] {
    let mut n = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut n);
    n
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn sign<T: TypedMessage>(
    wallet: &LocalWallet,
    message: T,
    domain: Eip712Domain,
    nonce: [u8; 16],
    signed_at: i64,
) -> Signed<T> {
    let mut signed = Signed {
        domain,
        message,
        nonce,
        signed_at,
        signature: String::new(),
    };
    let hash = signed.hash();
    let sig = wallet.sign_message(hash).await.unwrap();
    signed.signature = format!("0x{}", sig);
    signed
}

async fn add_moderator(pool: &PgPool, address: &str) {
    sqlx::query("INSERT INTO moderators (address, added_at) VALUES ($1, $2)")
        .bind(address.to_ascii_lowercase())
        .bind(now())
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn moderator_gate_and_schedule_and_settings_flow() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping moderator flow: set CATALYRST_EVENTS_TEST_PG to run");
        return;
    };
    let domain = domains::events();

    let moderator = mk_wallet(11);
    let random = mk_wallet(22);
    let victim = mk_wallet(33);

    // --- moderators-table gate ---
    assert!(!is_moderator(&pool, &addr(&moderator)).await.unwrap());
    require_moderator(&pool, &addr(&moderator))
        .await
        .expect_err("non-moderator must be rejected");
    add_moderator(&pool, &addr(&moderator)).await;
    assert!(is_moderator(&pool, &addr(&moderator)).await.unwrap());
    require_moderator(&pool, &addr(&moderator))
        .await
        .expect("moderator must pass");

    // --- schedule create then update by the moderator ---
    let create_msg = ScheduleUpsert {
        schedule_id: None,
        name: "MVMF".into(),
        description: Some("festival".into()),
        image: None,
        theme: Some("mvmf_2022".into()),
        background: vec!["#fff".into()],
        active_since: now(),
        active_until: now() + 86_400,
        active: true,
        signed_at: now(),
    };
    let create = sign(&moderator, create_msg, domain.clone(), rand_nonce(), now()).await;
    let (applied, sched) = apply::apply_schedule(&pool, &create, &addr(&moderator), None)
        .await
        .unwrap();
    assert!(applied.fresh);
    let sched_id = sched["id"].as_str().unwrap().to_string();
    assert_eq!(sched["name"], "MVMF");
    assert_eq!(sched["theme"], "mvmf_2022");

    // re-applying the same signed action is a no-op dedup (fresh == false).
    let (again, _) = apply::apply_schedule(&pool, &create, &addr(&moderator), None)
        .await
        .unwrap();
    assert!(!again.fresh);

    let update_msg = ScheduleUpsert {
        schedule_id: Some(sched_id.clone()),
        name: "MVMF 2".into(),
        description: None,
        image: None,
        theme: None,
        background: vec![],
        active_since: now(),
        active_until: now() + 100,
        active: false,
        signed_at: now(),
    };
    let update = sign(&moderator, update_msg, domain.clone(), rand_nonce(), now()).await;
    let (_, updated) = apply::apply_schedule(&pool, &update, &addr(&moderator), None)
        .await
        .unwrap();
    assert_eq!(updated["id"], sched_id);
    assert_eq!(updated["name"], "MVMF 2");
    assert_eq!(updated["active"], false);

    // --- moderator edits another user's profile settings ---
    let settings_msg = ProfileSettingsUpdate {
        target: addr(&victim),
        email: Some("v@x.io".into()),
        email_verified: Some(true),
        use_local_time: None,
        notify_by_email: Some(true),
        notify_by_browser: None,
        permissions: Some(vec!["edit_any_event".into()]),
        signed_at: now(),
    };
    let signed_settings = sign(&moderator, settings_msg, domain.clone(), rand_nonce(), now()).await;
    let (_, settings) =
        apply::apply_profile_settings(&pool, &signed_settings, &addr(&moderator), None)
            .await
            .unwrap();
    assert_eq!(settings["user"], addr(&victim).to_ascii_lowercase());
    assert_eq!(settings["email"], "v@x.io");
    assert_eq!(settings["permissions"][0], "edit_any_event");

    // listing reflects the materialised row.
    let list = apply::list_settings(&pool).await.unwrap();
    assert_eq!(list.len(), 1);

    // a non-moderator self-edit is allowed by policy (target == signer); the
    // moderator gate is only consulted for cross-user edits.
    let self_msg = ProfileSettingsUpdate {
        target: addr(&random),
        email: Some("me@x.io".into()),
        email_verified: None,
        use_local_time: Some(false),
        notify_by_email: None,
        notify_by_browser: None,
        permissions: None,
        signed_at: now(),
    };
    let self_signed = sign(&random, self_msg, domain.clone(), rand_nonce(), now()).await;
    let (_, self_settings) =
        apply::apply_profile_settings(&pool, &self_signed, &addr(&random), None)
            .await
            .unwrap();
    assert_eq!(self_settings["email"], "me@x.io");
    assert_eq!(self_settings["use_local_time"], false);

    cleanup(&admin_url, &schema).await;
}
