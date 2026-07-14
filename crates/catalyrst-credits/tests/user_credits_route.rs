mod common;

use axum::extract::{Path, State};
use axum::http::HeaderMap;

use catalyrst_credits::handlers::users::user_credits;
use catalyrst_credits::http::ApiError;

#[tokio::test]
async fn unauthenticated_is_a_400_invalid_auth_chain() {
    let state = common::test_state(common::lazy_pool(), false);
    let err = user_credits(
        State(state),
        Path("0x8d6f63e382d73cf53858864f673f39e9ff915a1e".to_string()),
        HeaderMap::new(),
    )
    .await
    .expect_err("missing auth chain must refuse");
    assert!(matches!(err, ApiError::InvalidAuthChain(_)), "got {err:?}");
    assert_eq!(common::status_of(err), 400);
}

#[tokio::test]
async fn signer_mismatch_is_a_403() {
    let state = common::test_state(common::lazy_pool(), false);
    let wallet = common::scratch_wallet();
    let other = "0x0000000000000000000000000000000000000009";
    let headers = common::signed_headers(&wallet, "get", &format!("/users/{other}/credits")).await;
    let err = user_credits(State(state), Path(other.to_string()), headers)
        .await
        .expect_err("signer must match the wallet in the path");
    assert!(matches!(err, ApiError::Forbidden(_)), "got {err:?}");
    assert_eq!(common::status_of(err), 403);
}

#[tokio::test]
async fn signed_wallet_reads_its_credits_end_to_end() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let wallet = common::scratch_wallet();
    let addr = common::wallet_addr(&wallet);
    sqlx::query(
        "INSERT INTO user_credits (address, available, earned_available) \
         VALUES ($1, 7.5, 2.5) \
         ON CONFLICT (address) DO UPDATE SET available = 7.5, earned_available = 2.5",
    )
    .bind(&addr)
    .execute(&pool)
    .await
    .unwrap();

    let app = catalyrst_credits::api_router().with_state(common::test_state(pool, false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let path = format!("/users/{addr}/credits");
    let headers = common::signed_headers(&wallet, "get", &path).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{local}{path}"))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(v["totalCredits"], 7.5);
    assert_eq!(v["totals"]["nonExpiring"], 7.5);
    assert_eq!(v["usd"]["credits"], 7);
    assert_eq!(v["usd"]["balanceCents"], 750);
    assert!(v["credits"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn unity_pack_listing_serves_the_client_path_and_shape() {
    let Some(pool) = common::pool().await else {
        return;
    };
    let app = catalyrst_credits::api_router().with_state(common::test_state(pool, false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let resp = reqwest::get(format!("http://{local}/credits/packs"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: serde_json::Value = resp.json().await.unwrap();
    let packs = v["packs"].as_array().expect("packs array");
    assert!(!packs.is_empty(), "migration-seeded packs expected");
    for p in packs {
        for key in ["id", "usd", "credits", "recommended", "order"] {
            assert!(p.get(key).is_some(), "pack missing {key}: {p}");
        }
    }
}

#[tokio::test]
async fn route_is_mounted_and_answers_the_adr44_envelope() {
    let app =
        catalyrst_credits::api_router().with_state(common::test_state(common::lazy_pool(), false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let resp = reqwest::get(format!(
        "http://{addr}/users/0x8d6f63e382d73cf53858864f673f39e9ff915a1e/credits"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 400);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "error": "Invalid Auth Chain",
            "message": "This endpoint requires a signed fetch request. See ADR-44.",
        })
    );
}
