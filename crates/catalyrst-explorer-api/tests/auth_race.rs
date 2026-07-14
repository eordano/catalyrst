//! Regression guard for item 2: get_identity consumption must be atomic.
//! Two concurrent GETs for the same identity id must yield exactly one 200
//! (the winner) and one 404 (the loser). Pre-change (clone-then-consume) both
//! could observe the record and both succeed; post-change DashMap::remove picks
//! exactly one winner. Deterministic post-change; a probabilistic falsifier
//! pre-change.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::modules::auth_api::IdentityRecord;
use catalyrst_explorer_api::{api_router, build_state};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_get_identity_consumes_exactly_once() {
    let cfg = Config::from_env().unwrap();
    let state = build_state(&cfg).await.unwrap();
    let app = api_router().with_state(state.clone());

    let id = "3fa85f64-5717-4562-b3fc-2c963f66afa6".to_string();

    for _round in 0..500u32 {
        // Wide chain widens the pre-change clone window between observe and consume.
        let chain: Vec<serde_json::Value> = (0..200)
            .map(|i| serde_json::json!({"type":"ECDSA_SIGNED_ENTITY","payload":format!("p-{i}")}))
            .collect();
        let now = chrono::Utc::now();
        state.auth_api.identities.insert(
            id.clone(),
            IdentityRecord {
                identity_id: id.clone(),
                identity: serde_json::json!({ "authChain": chain }),
                ip_address: String::new(),
                is_mobile: false,
                created_at: now,
                expiration: now + chrono::Duration::seconds(600),
            },
        );

        let a = app.clone();
        let b = app.clone();
        let id_a = id.clone();
        let id_b = id.clone();
        let (ra, rb) = tokio::join!(
            async move {
                a.oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/auth/identities/{id_a}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            },
            async move {
                b.oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/auth/identities/{id_b}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            },
        );

        let ok = (ra == StatusCode::OK) as u8 + (rb == StatusCode::OK) as u8;
        assert_eq!(
            ok, 1,
            "round {_round}: expected exactly one 200; got a={ra} b={rb}"
        );
    }
}
