//! /denylist.json is served from a primed in-memory cache, not by re-reading +
//! re-parsing the file on every request.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::modules::blocklist::Denylist;
use catalyrst_explorer_api::{api_router, build_state};

static ADMIN_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn get_json(app: &Router, path: &str) -> serde_json::Value {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn denylist_is_served_from_cache_not_disk() {
    let dir = std::env::temp_dir().join(format!("catalyrst-denylist-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("denylist.json");
    std::fs::write(&path, r#"{"users":["0xaaa","0xbbb","0xccc"]}"#).unwrap();

    let mut cfg = Config::from_env().unwrap();
    cfg.blocklist_path = path.to_str().unwrap().to_string();
    let state = build_state(&cfg).await.unwrap();
    let app = api_router().with_state(state);

    let body1 = get_json(&app, "/denylist.json").await;
    assert_eq!(body1["users"].as_array().unwrap().len(), 3);

    std::fs::remove_file(&path).unwrap();
    let body2 = get_json(&app, "/denylist.json").await;
    assert_eq!(
        body2["users"].as_array().unwrap().len(),
        3,
        "second call read disk instead of cache"
    );
}

#[tokio::test]
async fn admin_reload_refreshes_cache_from_disk() {
    let _env = ADMIN_ENV.lock().await;
    std::env::set_var("CATALYRST_EXPLORER_API_ADMIN_TOKEN", "test-token");
    let dir =
        std::env::temp_dir().join(format!("catalyrst-denylist-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("denylist.json");
    std::fs::write(&path, r#"{"users":["0xaaa","0xbbb"]}"#).unwrap();

    let mut cfg = Config::from_env().unwrap();
    cfg.blocklist_path = path.to_str().unwrap().to_string();
    let state = build_state(&cfg).await.unwrap();
    let app = api_router().with_state(state);

    let body1 = get_json(&app, "/denylist.json").await;
    assert_eq!(body1["users"].as_array().unwrap().len(), 2);

    std::fs::write(&path, r#"{"users":["0xaaa","0xbbb","0xccc","0xddd"]}"#).unwrap();
    let body_stale = get_json(&app, "/denylist.json").await;
    assert_eq!(body_stale["users"].as_array().unwrap().len(), 2);

    let reload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/blocklist/reload")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reload.status(), StatusCode::OK);

    let body2 = get_json(&app, "/denylist.json").await;
    assert_eq!(
        body2["users"].as_array().unwrap().len(),
        4,
        "reload did not refresh the cache from disk"
    );

    std::env::remove_var("CATALYRST_EXPLORER_API_ADMIN_TOKEN");
}

async fn post_json(app: &Router, path: &str, body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", "Bearer test-token")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn admin_add_and_remove_edit_memory_and_persist_without_rereading_the_file() {
    let _env = ADMIN_ENV.lock().await;
    std::env::set_var("CATALYRST_EXPLORER_API_ADMIN_TOKEN", "test-token");
    let dir = std::env::temp_dir().join(format!("catalyrst-denylist-edit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("denylist.json");
    std::fs::write(&path, r#"{"users":["0xAAA"]}"#).unwrap();

    let mut cfg = Config::from_env().unwrap();
    cfg.blocklist_path = path.to_str().unwrap().to_string();
    let state = build_state(&cfg).await.unwrap();
    let app = api_router().with_state(state.clone());

    let (status, body) = post_json(&app, "/admin/blocklist/add", r#"{"wallet":" 0xaaa "}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["added"], false);
    assert_eq!(body["count"], 1);

    std::fs::write(&path, r#"{"users":["0xzzz","0xyyy","0xxxx"]}"#).unwrap();
    let (_, body) = post_json(&app, "/admin/blocklist/add", r#"{"wallet":"0xBBB"}"#).await;
    assert_eq!(body["added"], true);
    assert_eq!(
        body["count"], 2,
        "the in-memory list is the source of truth"
    );
    let on_disk: Denylist = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(on_disk.users.len(), 2);
    assert!(state.denylist.read().contains("0xbbb"));

    let (_, body) = post_json(&app, "/admin/blocklist/remove", r#"{"wallet":"0xAAA"}"#).await;
    assert_eq!(body["removed"], true);
    assert_eq!(body["count"], 1);
    let (_, body) = post_json(&app, "/admin/blocklist/remove", r#"{"wallet":"0xAAA"}"#).await;
    assert_eq!(body["removed"], false);
    assert_eq!(
        get_json(&app, "/denylist.json").await["users"][0]["wallet"],
        "0xbbb"
    );
    std::env::remove_var("CATALYRST_EXPLORER_API_ADMIN_TOKEN");
}
