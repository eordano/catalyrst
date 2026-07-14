//! Item 4: /denylist.json is served from a primed in-memory cache, not by
//! re-reading + re-parsing the file on every request.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::{api_router, build_state};

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

    // Yank the backing file. A cached serve still returns 3; a disk read returns 0.
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

    // Grow the file on disk; the cache should not change until a reload.
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
