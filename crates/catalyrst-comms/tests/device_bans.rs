use std::time::Duration;

use catalyrst_comms::ports::player_connection::{
    PlayerConnectionComponent, UpsertPlayerConnection,
};
use catalyrst_comms::ports::user_bans::{CreateBan, UserBansComponent};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_COMMS_TEST_PG").ok()
}

fn unique_schema() -> String {
    format!("test_comms_{}", Uuid::new_v4().simple())
}

async fn setup_db() -> Option<(PgPool, String, String)> {
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

    apply_migration(&pool, include_str!("../migrations/0001_comms.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0002_user_moderation.sql"),
    )
    .await;
    apply_migration(
        &pool,
        include_str!("../migrations/0003_private_messages_privacy.sql"),
    )
    .await;
    apply_migration(&pool, include_str!("../migrations/0004_mls_messaging.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0005_published_events.sql"),
    )
    .await;
    apply_migration(
        &pool,
        include_str!("../migrations/0006_player_connection_and_device_bans.sql"),
    )
    .await;
    apply_migration(
        &pool,
        include_str!("../migrations/0007_community_voice_chat_sid.sql"),
    )
    .await;

    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_line_comments(sql);
    let mut buf = String::new();
    let mut in_func = false;
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.contains("$$ LANGUAGE plpgsql;") {
            in_func = false;
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
            continue;
        }
        if trimmed.contains("CREATE OR REPLACE FUNCTION") || trimmed.contains("CREATE FUNCTION") {
            in_func = true;
        }
        if !in_func && trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
            .execute(pool)
            .await
            .expect("trailing sql");
    }
}

fn strip_line_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        if line.trim_start().starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
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

#[tokio::test]
async fn upsert_coalesce_preserves_existing_nonnull_on_null() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping upsert_coalesce_preserves_existing_nonnull_on_null: set CATALYRST_COMMS_TEST_PG to run"
        );
        return;
    };
    let pc = PlayerConnectionComponent::new(pool.clone());
    let addr = "0x1111111111111111111111111111111111111111";

    pc.upsert(UpsertPlayerConnection {
        address: addr.into(),
        ip_address: Some("203.0.113.7".into()),
        device_id: Some("device-abc".into()),
    })
    .await
    .unwrap();
    assert_eq!(
        pc.get_device_id(addr).await.unwrap().as_deref(),
        Some("device-abc")
    );

    pc.upsert(UpsertPlayerConnection {
        address: addr.into(),
        ip_address: None,
        device_id: None,
    })
    .await
    .unwrap();
    let (ip, dev): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT ip_address, device_id FROM player_connection_info WHERE address = $1",
    )
    .bind(addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ip.as_deref(),
        Some("203.0.113.7"),
        "null incoming IP must not clear the stored IP"
    );
    assert_eq!(
        dev.as_deref(),
        Some("device-abc"),
        "null incoming device id must not clear the stored device id"
    );

    pc.upsert(UpsertPlayerConnection {
        address: addr.into(),
        ip_address: Some("198.51.100.4".into()),
        device_id: Some("device-xyz".into()),
    })
    .await
    .unwrap();
    let (ip2, dev2): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT ip_address, device_id FROM player_connection_info WHERE address = $1",
    )
    .bind(addr)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ip2.as_deref(), Some("198.51.100.4"));
    assert_eq!(dev2.as_deref(), Some("device-xyz"));

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn create_ban_snapshots_players_recorded_device_id() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping create_ban_snapshots_players_recorded_device_id: set CATALYRST_COMMS_TEST_PG to run"
        );
        return;
    };
    let pc = PlayerConnectionComponent::new(pool.clone());
    let bans = UserBansComponent::new(pool.clone());
    let victim = "0x1111111111111111111111111111111111111111";
    let moderator = "0x9999999999999999999999999999999999999999";

    pc.upsert(UpsertPlayerConnection {
        address: victim.into(),
        ip_address: Some("203.0.113.7".into()),
        device_id: Some("dev-snap".into()),
    })
    .await
    .unwrap();

    let snapshot = pc.get_device_id(&victim.to_lowercase()).await.unwrap();
    assert_eq!(snapshot.as_deref(), Some("dev-snap"));

    let ban = bans
        .create_ban(CreateBan {
            banned_address: victim.into(),
            banned_by: moderator.into(),
            reason: "abuse".into(),
            custom_message: None,
            banned_device_id: snapshot,
            duration_ms: None,
        })
        .await
        .expect("create_ban");
    assert_eq!(ban.banned_device_id.as_deref(), Some("dev-snap"));

    let stored: (Option<String>,) =
        sqlx::query_as("SELECT banned_device_id FROM user_bans WHERE banned_address = $1")
            .bind(victim)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0.as_deref(), Some("dev-snap"));

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn is_banned_for_connection_catches_wallet_switch_and_address_match() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping is_banned_for_connection_catches_wallet_switch_and_address_match: set CATALYRST_COMMS_TEST_PG to run"
        );
        return;
    };
    let bans = UserBansComponent::new(pool.clone());
    let banned_wallet = "0x1111111111111111111111111111111111111111";
    let fresh_wallet = "0x2222222222222222222222222222222222222222";
    let moderator = "0x9999999999999999999999999999999999999999";
    let device = "shared-device-42";

    bans.create_ban(CreateBan {
        banned_address: banned_wallet.into(),
        banned_by: moderator.into(),
        reason: "abuse".into(),
        custom_message: None,
        banned_device_id: Some(device.into()),
        duration_ms: None,
    })
    .await
    .expect("create_ban");

    assert!(
        bans.is_banned_for_connection(fresh_wallet, Some(device))
            .await
            .unwrap(),
        "ban evasion by switching wallet on the same device must be caught"
    );

    assert!(!bans
        .is_banned_for_connection(fresh_wallet, Some("some-other-device"))
        .await
        .unwrap());
    assert!(!bans
        .is_banned_for_connection(fresh_wallet, None)
        .await
        .unwrap());

    assert!(bans
        .is_banned_for_connection(banned_wallet, None)
        .await
        .unwrap());
    assert!(bans
        .is_banned_for_connection(banned_wallet, Some(device))
        .await
        .unwrap());

    cleanup(&admin_url, &schema).await;
}
