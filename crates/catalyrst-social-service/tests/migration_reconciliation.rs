use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::migrate::Migrator;
use sqlx::{PgPool, Row};
use uuid::Uuid;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

async fn schema_pool(prefix: &str) -> Option<ScratchSchema> {
    ScratchSchema::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", prefix).await
}

async fn applied_versions(pool: &PgPool) -> Vec<i64> {
    sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .expect("read _sqlx_migrations")
}

#[tokio::test]
async fn fresh_database_runs_the_full_reconciled_history() {
    let Some(scratch) = schema_pool("cg_social_migfresh").await else {
        return;
    };
    let pool = scratch.pool.clone();

    MIGRATOR.run(&pool).await.expect("full migration run");

    let versions = applied_versions(&pool).await;
    assert_eq!(versions, (1..=12).collect::<Vec<i64>>());
    for table in [
        "communities",
        "community_members",
        "friendships",
        "blocks",
        "user_mutes",
        "private_voice_chats",
        "friend_messages",
        "referral_progress",
        "referral_progress_viewed",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT 1 FROM {table} LIMIT 0"
        )))
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("table {table} missing after migration: {e}"));
    }
    let has_expires: Option<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = 'private_voice_chats' AND column_name = 'expires_at'",
    )
    .fetch_optional(&pool)
    .await
    .expect("introspect");
    assert!(
        has_expires.is_none(),
        "0010 must leave private_voice_chats without expires_at"
    );

    let has_requested_index: Option<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes \
         WHERE schemaname = current_schema() AND tablename = 'friendships' \
           AND indexname = 'friendships_address_requested'",
    )
    .fetch_optional(&pool)
    .await
    .expect("introspect friendships indexes");
    assert!(
        has_requested_index.is_some(),
        "0011 must index the address_requested arm so friend reads stop scanning the table"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn existing_deployments_converge_without_data_loss() {
    let Some(scratch) = schema_pool("cg_social_migconverge").await else {
        return;
    };
    let pool = scratch.pool.clone();

    for migration in MIGRATOR.iter().filter(|m| m.version <= 7) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("apply {} raw: {e}", m_label(migration)));
    }
    sqlx::raw_sql(
        "CREATE TABLE _sqlx_migrations (
             version BIGINT PRIMARY KEY,
             description TEXT NOT NULL,
             installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
             success BOOLEAN NOT NULL,
             checksum BYTEA NOT NULL,
             execution_time BIGINT NOT NULL
         )",
    )
    .execute(&pool)
    .await
    .expect("create _sqlx_migrations");
    for migration in MIGRATOR.iter().filter(|m| m.version <= 7) {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES ($1, $2, TRUE, $3, 0)",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref())
        .execute(&pool)
        .await
        .expect("stamp version");
    }

    let community_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address) VALUES ($1, 'converge', 'kept', '0xabc')",
    )
    .bind(community_id)
    .execute(&pool)
    .await
    .expect("seed community");

    for migration in MIGRATOR.iter().filter(|m| m.version >= 8) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .unwrap_or_else(|e| {
                panic!("simulate social import via {} raw: {e}", m_label(migration))
            });
    }
    let friendship_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO friendships (id, address_requester, address_requested, is_active) VALUES ($1, '0xaa', '0xbb', TRUE)",
    )
    .bind(friendship_id)
    .execute(&pool)
    .await
    .expect("seed imported friendship");

    MIGRATOR.run(&pool).await.expect("converging migration run");

    let versions = applied_versions(&pool).await;
    assert_eq!(versions, (1..=12).collect::<Vec<i64>>());
    let kept: String = sqlx::query("SELECT description FROM communities WHERE id = $1")
        .bind(community_id)
        .fetch_one(&pool)
        .await
        .expect("community row survives")
        .get(0);
    assert_eq!(kept, "kept");
    let active: bool = sqlx::query("SELECT is_active FROM friendships WHERE id = $1")
        .bind(friendship_id)
        .fetch_one(&pool)
        .await
        .expect("imported friendship survives")
        .get(0);
    assert!(active);

    MIGRATOR.run(&pool).await.expect("re-run is a no-op");

    scratch.drop().await;
}

fn m_label(m: &sqlx::migrate::Migration) -> String {
    format!("{:04}_{}", m.version, m.description)
}
