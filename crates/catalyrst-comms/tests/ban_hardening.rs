use std::sync::Arc;
use std::time::Duration;

use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::{BanWriteError, CreateBan, UserBansComponent};
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
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
        .max_connections(8)
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

fn test_state(pool: PgPool) -> AppState {
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        names: NamesComponent::new(None, "squid_marketplace".into()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool: None,
        dapps_pool: None,
        dapps_schema: "squid_marketplace".into(),
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: "http://127.0.0.1:1".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_host: String::new(),
        livekit_ws_url: String::new(),
        livekit_api_key: String::new(),
        livekit_api_secret: String::new(),
        livekit_webhook_key: None,
        livekit_configured: false,
        livekit_token_ttl_secs: 600,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
    })
}

#[tokio::test]
async fn concurrent_create_ban_yields_single_active_ban() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping concurrent_create_ban_yields_single_active_ban: set CATALYRST_COMMS_TEST_PG to run"
        );
        return;
    };
    let victim = "0x1111111111111111111111111111111111111111";
    let moderator = "0x9999999999999999999999999999999999999999";

    let mut handles = Vec::new();
    for _ in 0..8 {
        let bans = UserBansComponent::new(pool.clone());
        handles.push(tokio::spawn(async move {
            bans.create_ban(CreateBan {
                banned_address: victim.into(),
                banned_by: moderator.into(),
                reason: "abuse".into(),
                custom_message: None,
                banned_device_id: None,
                duration_ms: None,
            })
            .await
        }));
    }

    let mut created = 0;
    let mut already_banned = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(_) => created += 1,
            Err(BanWriteError::AlreadyBanned(addr)) => {
                assert_eq!(addr, victim);
                already_banned += 1;
            }
            Err(BanWriteError::Db(e)) => panic!("unexpected db error: {e}"),
        }
    }
    assert_eq!(created, 1, "exactly one concurrent ban must win");
    assert_eq!(already_banned, 7);

    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_bans \
         WHERE banned_address = $1 AND lifted_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(victim)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        active, 1,
        "the advisory lock must prevent duplicate active bans"
    );

    cleanup(&admin_url, &schema).await;
}

#[tokio::test]
async fn scene_ban_rejects_admin_target() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping scene_ban_rejects_admin_target: set CATALYRST_COMMS_TEST_PG to run");
        return;
    };
    let state = test_state(pool.clone());
    let place_id = "place-1";
    let admin = "0xAAAA111111111111111111111111111111111111";
    let owner = "0x9999999999999999999999999999999999999999";
    let bystander = "0x2222222222222222222222222222222222222222";

    state.scene_admin.add(place_id, admin, owner).await.unwrap();

    let err =
        catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(&state, place_id, admin)
            .await
            .expect_err("banning a scene admin must be rejected");
    assert_eq!(err.code, 400);
    assert_eq!(err.message, "Cannot ban this address");

    catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(&state, place_id, bystander)
        .await
        .expect("a regular user must remain bannable");

    cleanup(&admin_url, &schema).await;
}
