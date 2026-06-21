use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, Signer};
use catalyrst_events::fed::apply;
use catalyrst_events::fed::authority::{is_moderator, require_moderator};
use catalyrst_events::fed::messages::{ProfileSettingsUpdate, ScheduleUpsert};
use catalyrst_events::ports::events::{EventListFilters, EventsComponent};
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{Signed, TypedMessage};
use rand::RngExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_EVENTS_TEST_PG").ok()
}

fn unique_schema() -> String {
    let b: [u8; 8] = rand::rng().random();
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
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            schema
        )))
        .execute(&admin)
        .await;
    }
}

fn mk_wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    PrivateKeySigner::from_slice(&key).expect("wallet from bytes")
}

fn addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

fn rand_nonce() -> [u8; 16] {
    rand::rng().random()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn sign<T: TypedMessage>(
    wallet: &PrivateKeySigner,
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
    let sig = wallet.sign_message(&hash).await.unwrap();
    signed.signature = sig.to_string();
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

    assert!(!is_moderator(&pool, &addr(&moderator)).await.unwrap());
    require_moderator(&pool, &addr(&moderator))
        .await
        .expect_err("non-moderator must be rejected");
    add_moderator(&pool, &addr(&moderator)).await;
    assert!(is_moderator(&pool, &addr(&moderator)).await.unwrap());
    require_moderator(&pool, &addr(&moderator))
        .await
        .expect("moderator must pass");

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
    let signed_settings = sign(
        &moderator,
        settings_msg,
        domain.clone(),
        rand_nonce(),
        now(),
    )
    .await;
    let (_, settings) =
        apply::apply_profile_settings(&pool, &signed_settings, &addr(&moderator), None)
            .await
            .unwrap();
    assert_eq!(settings["user"], addr(&victim).to_ascii_lowercase());
    assert_eq!(settings["email"], "v@x.io");
    assert_eq!(settings["permissions"][0], "edit_any_event");

    let list = apply::list_settings(&pool).await.unwrap();
    assert_eq!(list.len(), 1);

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

async fn create_event_read_fixtures(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS event ( \
           id text PRIMARY KEY, \
           name text NOT NULL, \
           start_at timestamptz, \
           finish_at timestamptz, \
           next_start_at timestamptz, \
           next_finish_at timestamptz, \
           duration_ms bigint, \
           recurrent boolean NOT NULL DEFAULT false, \
           highlighted boolean NOT NULL DEFAULT false, \
           trending boolean NOT NULL DEFAULT false, \
           approved boolean NOT NULL DEFAULT false, \
           attending boolean, \
           community_id text, \
           user_creator text, \
           coordinates_x integer, \
           coordinates_y integer, \
           description text, \
           raw jsonb NOT NULL, \
           fetched_at timestamptz NOT NULL DEFAULT now() )",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS event_attendance_local ( \
           event_id text NOT NULL, \
           signer text NOT NULL, \
           signed_payload jsonb NOT NULL DEFAULT '{}'::jsonb, \
           action text NOT NULL, \
           signed_at timestamptz NOT NULL DEFAULT now(), \
           PRIMARY KEY (event_id, signer) )",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_event(pool: &PgPool, id: &str, creator: &str, deleted_by_admin: bool) {
    let raw = serde_json::json!({
        "user": creator,
        "deleted_by_admin": deleted_by_admin,
    });
    sqlx::query(
        "INSERT INTO event \
           (id, name, next_start_at, next_finish_at, approved, user_creator, \
            coordinates_x, coordinates_y, raw) \
         VALUES ($1, $2, now() - interval '1 hour', now() + interval '1 hour', \
                 true, $3, 0, 0, $4)",
    )
    .bind(id)
    .bind(format!("event {id}"))
    .bind(creator)
    .bind(raw)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn soft_deleted_events_hidden_from_all_reads() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping soft-delete read filter: set CATALYRST_EVENTS_TEST_PG to run");
        return;
    };
    create_event_read_fixtures(&pool).await;

    let creator = "0xabc0000000000000000000000000000000000001";
    insert_event(&pool, "ev-visible", creator, false).await;
    insert_event(&pool, "ev-deleted", creator, true).await;

    let events = EventsComponent::new(pool.clone());

    let (records, total) = events
        .list(&EventListFilters {
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert!(
        ids.contains(&"ev-visible"),
        "visible event missing: {ids:?}"
    );
    assert!(
        !ids.contains(&"ev-deleted"),
        "soft-deleted event leaked into list: {ids:?}"
    );
    assert_eq!(total, 1, "count(*) must also exclude the soft-deleted row");

    assert!(events.exists_visible("ev-visible", creator).await.unwrap());
    assert!(
        !events.exists_visible("ev-deleted", creator).await.unwrap(),
        "soft-deleted event must be invisible even to its owner"
    );

    let sitemap = events.sitemap_event_ids(0).await.unwrap();
    assert!(sitemap.contains(&"ev-visible".to_string()));
    assert!(
        !sitemap.contains(&"ev-deleted".to_string()),
        "soft-deleted event leaked into sitemap: {sitemap:?}"
    );

    for id in ["ev-visible", "ev-deleted"] {
        sqlx::query(
            "INSERT INTO event_attendance_local (event_id, signer, action) \
             VALUES ($1, $2, 'going')",
        )
        .bind(id)
        .bind(creator)
        .execute(&pool)
        .await
        .unwrap();
    }
    let attending = events.attending(creator).await.unwrap();
    let att_ids: Vec<&str> = attending.iter().map(|r| r.id.as_str()).collect();
    assert!(
        att_ids.contains(&"ev-visible"),
        "visible event missing from attending: {att_ids:?}"
    );
    assert!(
        !att_ids.contains(&"ev-deleted"),
        "soft-deleted event leaked into attending: {att_ids:?}"
    );

    cleanup(&admin_url, &schema).await;
}

async fn insert_event_status(
    pool: &PgPool,
    id: &str,
    creator: &str,
    approved: bool,
    rejected: bool,
) {
    let raw = serde_json::json!({
        "user": creator,
        "rejected": rejected,
    });
    sqlx::query(
        "INSERT INTO event \
           (id, name, next_start_at, next_finish_at, approved, user_creator, \
            coordinates_x, coordinates_y, raw) \
         VALUES ($1, $2, now() - interval '1 hour', now() + interval '1 hour', \
                 $3, $4, 0, 0, $5)",
    )
    .bind(id)
    .bind(format!("event {id}"))
    .bind(approved)
    .bind(creator)
    .bind(raw)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn owner_filter_returns_all_statuses_for_auth_user() {
    let Some((pool, schema, admin_url)) = setup().await else {
        eprintln!("skipping owner filter: set CATALYRST_EVENTS_TEST_PG to run");
        return;
    };
    create_event_read_fixtures(&pool).await;

    let owner = "0xaaa0000000000000000000000000000000000001";
    let other = "0xbbb0000000000000000000000000000000000002";

    insert_event_status(&pool, "own-approved", owner, true, false).await;
    insert_event_status(&pool, "own-pending", owner, false, false).await;
    insert_event_status(&pool, "own-rejected", owner, false, true).await;
    insert_event_status(&pool, "other-approved", other, true, false).await;

    let events = EventsComponent::new(pool.clone());

    let (records, total) = events
        .list(&EventListFilters {
            limit: 100,
            owner: true,
            user: Some(owner.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["own-approved", "own-pending", "own-rejected"],
        "owner listing must return all own statuses and exclude other users"
    );
    assert_eq!(total, 3, "owner count(*) must agree with the returned set");

    let (public, public_total) = events
        .list(&EventListFilters {
            limit: 100,
            user: Some(owner.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut public_ids: Vec<&str> = public.iter().map(|r| r.id.as_str()).collect();
    public_ids.sort();
    assert_eq!(
        public_ids,
        vec!["other-approved", "own-approved"],
        "non-owner path must surface only approved events"
    );
    assert_eq!(public_total, 2);

    cleanup(&admin_url, &schema).await;
}
