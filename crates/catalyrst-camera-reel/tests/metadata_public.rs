use std::path::Path as FsPath;
use std::sync::Arc;
use std::time::Duration;

use axum::body::to_bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_camera_reel::config::Config;
use catalyrst_camera_reel::dto::{Image, Metadata};
use catalyrst_camera_reel::handlers::images::get_metadata;
use catalyrst_camera_reel::http::ApiError;
use catalyrst_camera_reel::ports::db::Database;
use catalyrst_camera_reel::ports::places::PlacesClient;
use catalyrst_camera_reel::ports::storage::ImageStore;
use catalyrst_camera_reel::{AppState, AppStateInner};

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_CAMERA_REEL_TEST_PG").ok()
}

fn unique_schema() -> String {
    format!("test_cr_{}", Uuid::new_v4().simple())
}

fn unique_dir() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "catalyrst-camera-reel-test-{}",
        Uuid::new_v4().simple()
    ));
    p
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
    apply_migration(
        &pool,
        include_str!("../migrations/20260609000000_camera_reel_images.sql"),
    )
    .await;
    apply_migration(
        &pool,
        include_str!("../migrations/20260619000000_camera_reel_review_status.sql"),
    )
    .await;
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

async fn mk_state(pool: PgPool, dir: &FsPath, db_url: String) -> AppState {
    let config = Config {
        http_host: "127.0.0.1".to_string(),
        http_port: 0,
        database_url: db_url,
        content_storage_dir: dir.to_string_lossy().into_owned(),
        api_url: "http://127.0.0.1:5149".to_string(),
        bucket_url: None,
        max_images_per_user: 500,
        places_api_url: "http://127.0.0.1:5134".to_string(),
        places_cache_ttl_seconds: 300,
        places_cache_max_size: 1000,
        admin_token: None,
    };
    let store = ImageStore::new(dir).await.expect("init image store");
    let places = PlacesClient::new(config.places_api_url.clone(), 300, 1000);
    Arc::new(AppStateInner {
        db: Database::new(pool),
        store,
        places,
        config,
    })
}

fn private_image(owner: &str) -> Image {
    let metadata = Metadata {
        user_name: "owner".to_string(),
        user_address: owner.to_string(),
        date_time: "1700000000".to_string(),
        ..Metadata::default()
    };
    Image {
        id: Uuid::new_v4().to_string(),
        url: "http://127.0.0.1:5149/api/images/deadbeef".to_string(),
        thumbnail_url: "http://127.0.0.1:5149/api/images/cafebabe".to_string(),
        is_public: false,
        metadata,
    }
}

#[tokio::test]
async fn private_image_metadata_is_returned_without_auth() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping private_image_metadata_is_returned_without_auth: \
             set CATALYRST_CAMERA_REEL_TEST_PG to run"
        );
        return;
    };
    let dir = unique_dir();
    let state = mk_state(pool, &dir, admin_url.clone()).await;

    let owner = "0x1111111111111111111111111111111111111111";
    let image = private_image(owner);
    let image_id = image.id.clone();
    state
        .db
        .insert_image(&image)
        .await
        .expect("insert private image");

    let resp = get_metadata(State(state.clone()), Path(image_id.clone()))
        .await
        .expect("get_metadata must succeed for a private image with no auth");

    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    let returned: Image = serde_json::from_slice(body.as_ref()).expect("parse Image json");
    assert_eq!(returned.id, image_id);
    assert!(
        !returned.is_public,
        "returned image must be the private one (is_public stays false)"
    );
    assert_eq!(returned.metadata.user_address, owner);

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn missing_image_metadata_is_not_found_not_unauthorized() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!(
            "skipping missing_image_metadata_is_not_found_not_unauthorized: \
             set CATALYRST_CAMERA_REEL_TEST_PG to run"
        );
        return;
    };
    let dir = unique_dir();
    let state = mk_state(pool, &dir, admin_url.clone()).await;

    let missing = Uuid::new_v4().to_string();
    let err = get_metadata(State(state.clone()), Path(missing))
        .await
        .expect_err("absent image must error");
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound for an absent image, got {err:?}"
    );

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}
