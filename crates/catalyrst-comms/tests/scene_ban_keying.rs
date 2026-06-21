use std::time::Duration;

use catalyrst_comms::ports::scene_bans::SceneBansComponent;
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
async fn world_ban_keys_on_resolved_scene_id_not_world_name() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping world_ban_keys_on_resolved_scene_id_not_world_name: set CATALYRST_COMMS_TEST_PG to run"
        );
        return;
    };
    let bans = SceneBansComponent::new(pool.clone());

    let world_name = "foo.eth";
    let resolved_scene_id = "bafkreiabcdef123";
    let user = "0x1111111111111111111111111111111111111111";
    let moderator = "0x9999999999999999999999999999999999999999";

    bans.ban(resolved_scene_id, user, moderator)
        .await
        .expect("ban");

    assert!(
        bans.is_banned(resolved_scene_id, user).await.unwrap(),
        "ban must be found when keyed on the resolved scene content-hash"
    );

    assert!(
        !bans.is_banned(world_name, user).await.unwrap(),
        "ban keyed on the scene content-hash must NOT be found under the raw world name"
    );

    assert!(bans
        .is_banned(resolved_scene_id, &user.to_uppercase())
        .await
        .unwrap());

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn listing_under_resolved_key_sees_hot_path_bans() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping listing_under_resolved_key_sees_hot_path_bans: set CATALYRST_COMMS_TEST_PG to run");
        return;
    };
    let bans = SceneBansComponent::new(pool.clone());

    let world_name = "foo.dcl.eth";
    let resolved_scene_id = "bafkreiabcdef123";
    let user = "0x3333333333333333333333333333333333333333";
    let moderator = "0x9999999999999999999999999999999999999999";

    bans.ban(resolved_scene_id, user, moderator)
        .await
        .expect("ban");

    assert_eq!(bans.count(resolved_scene_id).await.unwrap(), 1);
    assert_eq!(
        bans.list_addresses_page(resolved_scene_id, 100, 0)
            .await
            .unwrap(),
        vec![user.to_string()]
    );

    assert_eq!(
        bans.count(world_name).await.unwrap(),
        0,
        "the realm-name key must no longer accumulate or surface bans"
    );
    assert!(bans
        .list_addresses_page(world_name, 100, 0)
        .await
        .unwrap()
        .is_empty());

    cleanup(&admin_url, &schema).await;
}
