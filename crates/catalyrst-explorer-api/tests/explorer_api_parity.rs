use std::collections::BTreeSet;
use std::future::IntoFuture;
use std::net::SocketAddr;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::modules::feature_flags::Variant;
use catalyrst_explorer_api::{api_router, build_state};

const FIXTURE: &str = include_str!("fixtures/about-upstream-interconnected.json");
const LIVE_ABOUT_URL: &str = "https://peer.interconnected.online/about";

async fn mock_json(route: &'static str, body: Value) -> SocketAddr {
    let app = axum::Router::new().route(
        route,
        axum::routing::get(move || {
            let body = body.clone();
            async move { axum::Json(body) }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(axum::serve(l, app).into_future());
    addr
}

async fn router_for(cfg: Config) -> Router {
    let state = build_state(&cfg).await.unwrap();
    api_router().with_state(state)
}

async fn get(app: &Router, path: &str, bearer: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(path);
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn diff_keys(
    path: &str,
    ours: &Value,
    theirs: &Value,
    allow: &BTreeSet<&str>,
    out: &mut Vec<String>,
) {
    match (ours, theirs) {
        (Value::Object(o), Value::Object(t)) => {
            for k in t.keys() {
                if !o.contains_key(k) {
                    out.push(format!("missing upstream key {path}.{k}"));
                }
            }
            for k in o.keys() {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if !t.contains_key(k) && !allow.contains(child.as_str()) {
                    out.push(format!("ours-only key {child}"));
                }
            }
            for (k, ov) in o {
                if let Some(tv) = t.get(k) {
                    let child = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    diff_keys(&child, ov, tv, allow, out);
                }
            }
        }
        (Value::Array(o), Value::Array(t)) => {
            let child = format!("{path}[]");
            for i in 0..o.len().min(t.len()) {
                diff_keys(&child, &o[i], &t[i], allow, out);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn about_matches_upstream_baseline() {
    let addr = mock_json(
        "/content/status",
        json!({
            "version": "8.0.3+rust",
            "commitHash": "b001118",
            "synchronizationStatus": { "synchronizationState": "Syncing" },
        }),
    )
    .await;

    let mut cfg = Config::from_env().unwrap();
    let base = format!("http://{addr}");
    cfg.catalyst_url = base.clone();
    cfg.public_base_url = Some(base.clone());
    let app = router_for(cfg).await;

    let (status, ours) = get(&app, "/about", None).await;
    assert_eq!(status, StatusCode::OK);

    let theirs: Value = serde_json::from_str(FIXTURE).unwrap();
    let allow: BTreeSet<&str> = ["configurations.localSceneParcels", "configurations.skybox"]
        .into_iter()
        .collect();
    let mut failures = Vec::new();
    diff_keys("", &ours, &theirs, &allow, &mut failures);
    assert!(
        failures.is_empty(),
        "about key drift vs baseline: {failures:?}"
    );

    let content_url = ours["content"]["publicUrl"].as_str().unwrap();
    let lambdas_url = ours["lambdas"]["publicUrl"].as_str().unwrap();
    assert_eq!(content_url, format!("{base}/content/"));
    assert_eq!(lambdas_url, format!("{base}/lambdas/"));
    assert!(content_url.ends_with("/content/"));
    assert!(lambdas_url.ends_with("/lambdas/"));
}

#[tokio::test]
async fn flags_document_is_unleash_proxy_shape() {
    std::env::remove_var("FEATURE_FLAGS_CONFIG_PATH");
    let cfg = Config::from_env().unwrap();
    let app = router_for(cfg).await;

    let (status, doc) = get(&app, "/explorer.json", None).await;
    assert_eq!(status, StatusCode::OK);

    let top: BTreeSet<String> = keys(&doc);
    let expected: BTreeSet<String> = ["flags".to_string(), "variants".to_string()]
        .into_iter()
        .collect();
    assert_eq!(
        top, expected,
        "top-level keys must be exactly flags+variants"
    );

    for (name, value) in doc["flags"].as_object().unwrap() {
        assert!(value.is_boolean(), "flag {name} is not a bool: {value}");
    }

    for (name, value) in doc["variants"].as_object().unwrap() {
        let parsed: Variant = serde_json::from_value(value.clone())
            .unwrap_or_else(|e| panic!("variant {name} does not fit typed Variant: {e}"));
        let round = serde_json::to_value(&parsed).unwrap();
        assert_eq!(
            &round, value,
            "variant {name} lost or gained fields on round-trip"
        );
    }
}

#[tokio::test]
async fn fixed_shapes_are_exact() {
    let cfg = Config::from_env().unwrap();
    let realm_name = cfg.realm_name.clone();
    let realm_url = cfg.public_realm_url.clone();
    let app = router_for(cfg).await;

    let (status, realms) = get(&app, "/realms", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        realms,
        json!([{ "serverName": realm_name, "url": realm_url, "usersCount": 0 }])
    );

    let (status, st) = get(&app, "/status", None).await;
    assert_eq!(status, StatusCode::OK);
    let st_keys: BTreeSet<String> = keys(&st);
    let want: BTreeSet<String> = ["version", "currentTime", "commitHash"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(st_keys, want);
    assert!(st["currentTime"].is_i64());
}

#[tokio::test]
async fn pending_nudges_is_exact_empty_shape() {
    let mut cfg = Config::from_env().unwrap();
    cfg.onboarding_api_key = Some("parity-key".to_string());
    let app = router_for(cfg).await;

    let (status, body) = get(&app, "/onboarding/pending-nudges", Some("parity-key")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "sequence": 1, "count": 0, "nudges": [] }));
}

#[tokio::test]
async fn hot_scenes_round_trips_typed_shape() {
    let scene = json!([
        {
            "id": "s1",
            "name": "Plaza",
            "baseCoords": [10, -5],
            "parcels": [[10, -5], [11, -5]],
            "usersTotalCount": 3,
            "realms": [{ "serverName": "loki", "usersCount": 3 }],
            "thumbnail": "https://example.test/t.png"
        },
        {
            "id": "s2",
            "name": "Void",
            "baseCoords": [0, 0],
            "parcels": [[0, 0]],
            "usersTotalCount": 0,
            "realms": []
        }
    ]);
    let addr = mock_json("/hot-scenes", scene.clone()).await;

    let mut cfg = Config::from_env().unwrap();
    cfg.hot_scenes_url = format!("http://{addr}/hot-scenes");
    let app = router_for(cfg).await;

    let (status, body) = get(&app, "/hot-scenes", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, scene);
    assert!(
        body[1].as_object().unwrap().get("thumbnail").is_none(),
        "absent thumbnail must be omitted, not null"
    );
}

#[tokio::test]
async fn upstream_baseline_is_current() {
    if std::env::var("EXPLORER_API_PARITY_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let live: Value = reqwest::get(LIVE_ABOUT_URL)
        .await
        .expect("live upstream fetch failed")
        .json()
        .await
        .expect("live upstream body was not JSON");
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let allow: BTreeSet<&str> = BTreeSet::new();
    let mut failures = Vec::new();
    diff_keys("", &fixture, &live, &allow, &mut failures);
    diff_keys("", &live, &fixture, &allow, &mut failures);
    assert!(
        failures.is_empty(),
        "upstream /about moved; recapture tests/fixtures/about-upstream-interconnected.json: {failures:?}"
    );
}
