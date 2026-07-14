// DB-gated end-to-end coverage for the scene-scoped delegation grant on GET /env/{key}
// (upstream f2eb3be): a valid delegation reads env values; env list/upsert/delete and
// unrelated signers stay denied. Set CATALYRST_WORLD_STORAGE_TEST_PG to run; each test
// works in a throwaway schema and drops it on the way out.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use catalyrst_crypto::Wallet;
use catalyrst_world_storage::config::{Config, NamespaceLimits, StorageCacheConfig};
use catalyrst_world_storage::delegation::STORAGE_DELEGATION_PREFIX;
use catalyrst_world_storage::storage::value_size_bytes;
use catalyrst_world_storage::{api_router, build_state, AppState};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

static COUNTER: AtomicU64 = AtomicU64::new(0);

const AUTHORITATIVE_KEY: &str =
    "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const WORKER_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const STRANGER_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

const WORLD: &str = "delegated.dcl.eth";
const SCENE_ID: &str = "bafkrei-delegated-scene";
const PARCEL: &str = "0,0";
const PLACE: &str = "22222222-2222-2222-2222-222222222222";
const KEY: &str = "api-key";
const SECRET: &str = "secret-api-key-12345";
const DENIED: &str = "Unauthorized: Signer is not authorized to perform operations on this world";

fn unique_schema() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("test_wstenv_{}_{}_{}", std::process::id(), nanos, n)
}

// Places resolves every lookup to PLACE; the worlds content server reports an owner
// unrelated to every test wallet, so owner/deployer checks always come back false.
async fn spawn_upstream_mock() -> String {
    let app = Router::new()
        .route(
            "/api/places",
            get(|| async { Json(json!({ "data": [{ "id": PLACE }] })) }),
        )
        .route(
            "/world/{name}/permissions",
            get(|| async {
                Json(json!({
                    "owner": "0x0000000000000000000000000000000000000001",
                    "permissions": { "deployment": { "type": "allow-list", "wallets": [] } }
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

struct Setup {
    app: Router,
    state: AppState,
    schema: String,
    admin_url: String,
}

async fn setup() -> Option<Setup> {
    let url = std::env::var("CATALYRST_WORLD_STORAGE_TEST_PG").ok()?;
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
    let database_url = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let mock_base = spawn_upstream_mock().await;
    let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let cfg = Config {
        http_host: "127.0.0.1".to_string(),
        http_port: 0,
        database_url,
        cors_allowed_origin_suffixes: vec![],
        encryption_key: [7u8; 32],
        authoritative_server_address: Some(authoritative.address()),
        authorized_addresses: vec![],
        eip1654_rpc_url: None,
        worlds_content_server_url: mock_base.clone(),
        lambdas_url: mock_base.clone(),
        places_url: mock_base,
        places_cache_ttl_seconds: 300,
        world_permission_cache_ttl_seconds: 30,
        storage_cache: StorageCacheConfig {
            enabled: true,
            ttl_seconds: 60,
            max_entries: 1000,
            max_value_bytes: 32_768,
        },
        env_limits: NamespaceLimits {
            max_value_size_bytes: 10_240,
            max_total_size_bytes: 262_144,
        },
        world_limits: NamespaceLimits {
            max_value_size_bytes: 524_288,
            max_total_size_bytes: 10_485_760,
        },
        player_limits: NamespaceLimits {
            max_value_size_bytes: 102_400,
            max_total_size_bytes: 1_048_576,
        },
    };
    let state = build_state(cfg)
        .await
        .expect("state builds and migrations apply");
    let app = api_router().with_state(state.clone());
    Some(Setup {
        app,
        state,
        schema,
        admin_url: url,
    })
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

async fn seed_secret(state: &AppState) {
    let enc = state.encryptor.encrypt(SECRET).unwrap();
    state
        .storage
        .env_upsert_with_quota(
            WORLD,
            PLACE,
            KEY,
            &enc,
            value_size_bytes(SECRET),
            state.cfg.env_limits,
        )
        .await
        .unwrap();
}

fn scene_metadata() -> String {
    json!({
        "realm": { "serverName": WORLD },
        "parcel": PARCEL,
        "sceneId": SCENE_ID,
        "signer": "dcl:authoritative-server"
    })
    .to_string()
}

fn scope_header(authoritative: &Wallet, ephemeral: &str, parcel: &str) -> String {
    let payload = format!(
        "{STORAGE_DELEGATION_PREFIX}\nEphemeral: {ephemeral}\nWorld: {WORLD}\nSceneId: {SCENE_ID}\nParcel: {parcel}\nExpiration: {}",
        (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()
    );
    let signature = authoritative.sign_message(payload.as_bytes()).unwrap();
    base64_encode(
        json!({ "payload": payload, "signature": signature })
            .to_string()
            .as_bytes(),
    )
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let mut acc: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            acc |= (b as u32) << (16 - 8 * i);
        }
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((acc >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ADR-44 signed fetch with a minimal SIGNER + ECDSA_SIGNED_ENTITY chain; the payload is
// lowercased exactly as verify_request rebuilds it.
fn signed_request(
    method: &str,
    path: &str,
    wallet: &Wallet,
    metadata: &str,
    scope: Option<&str>,
    body: Option<&str>,
) -> Request<Body> {
    let ts = chrono::Utc::now().timestamp_millis().to_string();
    let payload = format!("{}:{}:{}:{}", method, path, ts, metadata).to_lowercase();
    let signature = wallet.sign_message(payload.as_bytes()).unwrap();
    let link0 = json!({ "type": "SIGNER", "payload": wallet.address(), "signature": "" });
    let link1 =
        json!({ "type": "ECDSA_SIGNED_ENTITY", "payload": payload, "signature": signature });
    let mut builder = Request::builder()
        .method(method.to_uppercase().as_str())
        .uri(path)
        .header("x-identity-auth-chain-0", link0.to_string())
        .header("x-identity-auth-chain-1", link1.to_string())
        .header("x-identity-timestamp", ts)
        .header("x-identity-metadata", metadata);
    if let Some(scope) = scope {
        builder = builder.header("x-authoritative-scope", scope);
    }
    match body {
        Some(b) => builder
            .header("content-type", "application/json")
            .header("content-length", b.len().to_string())
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[tokio::test]
async fn scoped_delegation_reads_env_values_and_nothing_else() {
    let Some(setup) = setup().await else {
        eprintln!("skipping scoped_delegation_reads_env_values_and_nothing_else: set CATALYRST_WORLD_STORAGE_TEST_PG to run");
        return;
    };
    seed_secret(&setup.state).await;

    let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let worker = Wallet::from_hex(WORKER_KEY).unwrap();
    let scope = scope_header(&authoritative, &worker.address(), PARCEL);
    let meta = scene_metadata();
    let value_path = format!("/env/{}", KEY);

    let (status, body) = send(
        &setup.app,
        signed_request("get", &value_path, &worker, &meta, Some(&scope), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "delegated env read: {body}");
    assert_eq!(body, json!({ "value": SECRET }));

    let (status, body) = send(
        &setup.app,
        signed_request("get", "/env", &worker, &meta, Some(&scope), None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a delegation must not list env keys: {body}"
    );
    assert_eq!(body["message"], DENIED);

    let (status, body) = send(
        &setup.app,
        signed_request(
            "put",
            &value_path,
            &worker,
            &meta,
            Some(&scope),
            Some(r#"{"value":"hijacked"}"#),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a delegation must not upsert env values: {body}"
    );
    assert_eq!(body["message"], DENIED);

    let (status, body) = send(
        &setup.app,
        signed_request("delete", &value_path, &worker, &meta, Some(&scope), None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a delegation must not delete env values: {body}"
    );
    assert_eq!(body["message"], DENIED);

    let stranger = Wallet::from_hex(STRANGER_KEY).unwrap();
    let (status, body) = send(
        &setup.app,
        signed_request("get", &value_path, &stranger, &meta, None, None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "an unrelated signer must stay denied: {body}"
    );
    assert_eq!(body["message"], DENIED);

    let (status, body) = send(
        &setup.app,
        signed_request("get", &value_path, &authoritative, &meta, None, None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorized-address reads must keep working: {body}"
    );
    assert_eq!(
        body,
        json!({ "value": SECRET }),
        "the denied write and delete must not have touched the value"
    );

    cleanup(&setup.admin_url, &setup.schema).await;
}

#[tokio::test]
async fn parcel_mismatched_claim_is_rejected() {
    let Some(setup) = setup().await else {
        eprintln!("skipping parcel_mismatched_claim_is_rejected: set CATALYRST_WORLD_STORAGE_TEST_PG to run");
        return;
    };
    seed_secret(&setup.state).await;

    let authoritative = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let worker = Wallet::from_hex(WORKER_KEY).unwrap();
    // The claim is bound to another parcel; the parcel pins the place_id, so one
    // scene's claim must never read another scene's env values.
    let scope = scope_header(&authoritative, &worker.address(), "10,-25");

    let (status, body) = send(
        &setup.app,
        signed_request(
            "get",
            &format!("/env/{}", KEY),
            &worker,
            &scene_metadata(),
            Some(&scope),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body,
        json!({ "error": "Not Authorized", "message": DENIED })
    );

    cleanup(&setup.admin_url, &setup.schema).await;
}
