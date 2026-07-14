use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

const PLACE: &str = "11111111-1111-1111-1111-111111111111";
const PLAYER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TABLES: [&str; 3] = ["world_storage", "player_storage", "env_variables"];

async fn setup_pool() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_WORLD_STORAGE_TEST_PG", "cg_wstmig").await?;
    sqlx::raw_sql(include_str!("../migrations/0007_world_storage_initial.sql"))
        .execute(&scratch.pool)
        .await
        .expect("0007 applies");
    Some(scratch)
}

async fn seed(pool: &PgPool, world: &str, key: &str) {
    sqlx::query(
        "INSERT INTO world_storage (world_name, place_id, key, value)
         VALUES ($1, $2::uuid, $3, '{\"seeded\":true}'::jsonb)",
    )
    .bind(world)
    .bind(PLACE)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO player_storage (world_name, place_id, player_address, key, value)
         VALUES ($1, $2::uuid, $3, $4, '\"v\"'::jsonb)",
    )
    .bind(world)
    .bind(PLACE)
    .bind(PLAYER)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO env_variables (world_name, place_id, key, value_enc)
         VALUES ($1, $2::uuid, $3, decode('00', 'hex'))",
    )
    .bind(world)
    .bind(PLACE)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
}

async fn world_names(pool: &PgPool, table: &str) -> Vec<String> {
    let mut names: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT world_name FROM {table}"
    )))
    .fetch_all(pool)
    .await
    .unwrap();
    names.sort();
    names
}

#[tokio::test]
async fn colliding_names_abort_the_migration_and_preserve_every_row() {
    let Some(scratch) = setup_pool().await else {
        return;
    };
    let pool = scratch.pool.clone();
    seed(&pool, "MyWorld.dcl.eth", "k").await;
    seed(&pool, "myworld.dcl.eth", "k").await;
    seed(&pool, "Solo.DCL.eth", "k").await;

    let err = sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("collision"), "{err}");
    assert!(err.contains("MyWorld.dcl.eth"), "{err}");
    assert!(err.contains("player_storage"), "{err}");
    assert!(err.contains("env_variables"), "{err}");

    for table in TABLES {
        assert_eq!(
            world_names(&pool, table).await,
            ["MyWorld.dcl.eth", "Solo.DCL.eth", "myworld.dcl.eth"],
            "{table}"
        );
    }

    let applied: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        applied, 8,
        "the failed lowercase migration (9) must not be recorded; 0001-0008 apply first"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn mixed_case_names_without_collisions_are_lowercased_in_place() {
    let Some(scratch) = setup_pool().await else {
        return;
    };
    let pool = scratch.pool.clone();
    seed(&pool, "MyWorld.dcl.eth", "k").await;
    seed(&pool, "Other.DCL.eth", "k2").await;

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    for table in TABLES {
        assert_eq!(
            world_names(&pool, table).await,
            ["myworld.dcl.eth", "other.dcl.eth"],
            "{table}"
        );
    }

    let value: String = sqlx::query_scalar(
        "SELECT value::text FROM world_storage WHERE world_name = $1 AND key = $2",
    )
    .bind("myworld.dcl.eth")
    .bind("k")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&value).unwrap(),
        serde_json::json!({"seeded": true})
    );

    sqlx::raw_sql(include_str!(
        "../migrations/0009_world_storage_lowercase_world_names.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    for table in TABLES {
        assert_eq!(
            world_names(&pool, table).await,
            ["myworld.dcl.eth", "other.dcl.eth"],
            "{table}"
        );
    }

    scratch.drop().await;
}
