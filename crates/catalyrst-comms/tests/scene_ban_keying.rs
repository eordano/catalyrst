use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_sceneban").await?;
    apply_migration(&scratch.pool, include_str!("../migrations/0001_comms.sql")).await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0002_user_moderation.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0003_private_messages_privacy.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0004_mls_messaging.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0005_published_events.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0006_player_connection_and_device_bans.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0007_community_voice_chat_sid.sql"),
    )
    .await;

    Some(scratch)
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

#[tokio::test]
async fn world_ban_keys_on_resolved_scene_id_not_world_name() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
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

    scratch.drop().await;
}

#[tokio::test]
async fn listing_under_resolved_key_sees_hot_path_bans() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
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

    scratch.drop().await;
}
