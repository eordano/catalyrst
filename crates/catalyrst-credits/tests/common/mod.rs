#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use ethers_signers::{LocalWallet, Signer};
use sha2::{Digest, Sha256};

use catalyrst_credits::auth_chain::build_payload;
use catalyrst_credits::ports::credits::CreditsComponent;
use catalyrst_credits::ports::pricing::PricingClient;
use catalyrst_credits::{AppState, AppStateInner};

static WALLET_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn scratch_wallet() -> LocalWallet {
    let mut h = Sha256::new();
    h.update(std::process::id().to_le_bytes());
    h.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes(),
    );
    h.update(WALLET_COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let key: [u8; 32] = h.finalize().into();
    LocalWallet::from_bytes(&key).expect("wallet from bytes")
}

pub fn wallet_addr(w: &LocalWallet) -> String {
    format!("{:#x}", w.address())
}

fn link_json(kind: &str, payload: &str, signature: &str) -> String {
    serde_json::json!({ "type": kind, "payload": payload, "signature": signature }).to_string()
}

pub async fn signed_headers(wallet: &LocalWallet, method: &str, path: &str) -> HeaderMap {
    let root_addr = wallet_addr(wallet);
    let ephemeral = scratch_wallet();
    let ephemeral_addr = wallet_addr(&ephemeral);

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = format!(
        "0x{}",
        wallet
            .sign_message(ephemeral_payload.as_bytes())
            .await
            .unwrap()
    );

    let ts_ms = chrono_now_ms();
    let canonical = build_payload(method, path, &ts_ms.to_string(), "{}");
    let entity_sig = format!(
        "0x{}",
        ephemeral.sign_message(canonical.as_bytes()).await.unwrap()
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-0"),
        HeaderValue::from_str(&link_json("SIGNER", &root_addr, "")).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-1"),
        HeaderValue::from_str(&link_json(
            "ECDSA_EPHEMERAL",
            &ephemeral_payload,
            &ephemeral_sig,
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&link_json("ECDSA_SIGNED_ENTITY", &canonical, &entity_sig)).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-timestamp"),
        HeaderValue::from_str(&ts_ms.to_string()).unwrap(),
    );
    headers
}

fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub async fn pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("CREDITS_TEST_PG_CONNECTION_STRING").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("test PG unreachable");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("test PG migrations failed");
    Some(pool)
}

pub fn lazy_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/never")
        .expect("lazy pool")
}

pub fn test_state(pool: sqlx::PgPool, mock_card: bool) -> AppState {
    test_state_with_market(pool, mock_card, "http://127.0.0.1:1", "secondary")
}

pub fn test_state_with_market(
    pool: sqlx::PgPool,
    mock_card: bool,
    market_base_url: &str,
    fulfillment_mode: &str,
) -> AppState {
    let http = reqwest::Client::new();
    Arc::new(AppStateInner {
        credits: CreditsComponent::new(pool.clone()),
        admin_token: None,
        captcha_provider: None,
        stripe: None,
        stripe_webhook_secret: None,
        mock_card,
        credits_currency: "usd".into(),
        pricing: PricingClient::new(
            http.clone(),
            market_base_url.into(),
            market_base_url.into(),
            0,
            3600,
        ),
        checkout_fulfillment_mode: fulfillment_mode.into(),
        require_purchase_intent: false,
        economy_base_url: "http://127.0.0.1:1".into(),
        economy_admin_token: Some("test-token".into()),
        escrow_address: Some("0x0000000000000000000000000000000000000001".into()),
        usage_grants_pool: Some(pool),
        economy_http: http,
        quote_cache: Default::default(),
    })
}

pub fn status_of(err: catalyrst_credits::http::ApiError) -> u16 {
    use axum::response::IntoResponse;
    err.into_response().status().as_u16()
}
