mod support;

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Bytes};
use axum::extract::{Extension, State};
use axum::http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use catalyrst_comms::assignment_fence::{
    FenceError, PgAssignmentFence, RealmAssignmentReader, RealmAssignmentSnapshot,
};
use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::handlers::island_refresh::{
    refresh, IslandRefreshResponse, ISLAND_REFRESH_CONTENT_TYPE, ISLAND_REFRESH_PATH,
    ISLAND_REFRESH_TTL_SECONDS,
};
use catalyrst_crypto::Wallet;
use prost::Message;
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};

const AUDIENCE: &str = "island-refresh-test";
const ROOT_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const SESSION_KEY: &str = "0x8f2a559490f9f44e45d6359dba5d9db30c4f4e3f6380b5d5205d9e8d5f970421";

struct Fixture {
    admin: PgPool,
    pool: PgPool,
    schema: String,
    url: String,
}

impl Fixture {
    async fn new() -> Option<Self> {
        let url = test_pg_url()?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap_or_else(|error| panic!("connect configured PG18: {error}"));
        let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
            .fetch_one(&admin)
            .await
            .expect("read PostgreSQL version");
        assert!(
            version >= 180_000,
            "island refresh test requires PostgreSQL 18"
        );
        let schema = format!("island_refresh_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .expect("create isolated schema");
        let options = PgConnectOptions::from_str(&url)
            .expect("parse PostgreSQL URL")
            .options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .expect("connect isolated schema");
        create_tables(&pool).await;
        let separator = if url.contains('?') { '&' } else { '?' };
        let scoped_url = format!("{url}{separator}options[search_path]={schema}");
        Some(Self {
            admin,
            pool,
            schema,
            url: scoped_url,
        })
    }

    async fn reader(&self) -> Arc<dyn RealmAssignmentReader> {
        Arc::new(
            PgAssignmentFence::connect(&self.url, AUDIENCE.into())
                .await
                .expect("connect assignment reader"),
        )
    }

    async fn finish(self) {
        self.pool.close().await;
        sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .expect("drop isolated schema");
        self.admin.close().await;
    }
}

fn test_pg_url() -> Option<String> {
    std::env::var("CATALYRST_ARCHIPELAGO_TEST_PG")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("CATALYRST_COMMS_TEST_PG")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| catalyrst_testgate::require_pg("CATALYRST_ARCHIPELAGO_TEST_PG"))
}

async fn create_tables(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE TABLE player_connection_info (
            address text PRIMARY KEY,
            device_id text
        );
        CREATE TABLE user_bans (
            banned_address text NOT NULL,
            banned_device_id text,
            lifted_at timestamptz,
            expires_at timestamptz
        );
        CREATE TABLE archipelago_v4_assignments (
            deployment_audience text NOT NULL,
            owner_address text NOT NULL,
            lane_key text NOT NULL,
            authority_incarnation text NOT NULL,
            assignment_revision bigint NOT NULL,
            owner_session text NOT NULL,
            owner_epoch bigint NOT NULL,
            fencing_token bigint NOT NULL,
            assignment_json jsonb,
            acknowledged_revision bigint NOT NULL DEFAULT 0,
            updated_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (deployment_audience, owner_address, lane_key)
        );",
    )
    .execute(pool)
    .await
    .expect("create refresh fixture tables");
}

async fn seed_assignment(pool: &PgPool, wallet: &str, session: &str, island: &str, revision: i64) {
    sqlx::query(
        "INSERT INTO archipelago_v4_assignments (
            deployment_audience, owner_address, lane_key, authority_incarnation,
            assignment_revision, owner_session, owner_epoch, fencing_token, assignment_json
         ) VALUES ($1, $2, 'realm', 'authority-refresh-test', $3, $4, 7, 11, $5)",
    )
    .bind(AUDIENCE)
    .bind(wallet)
    .bind(revision)
    .bind(session)
    .bind(json!({
        "islandId": island,
        "connectionString": "livekit:expired",
        "tokenExpiresAtUnix": 1
    }))
    .execute(pool)
    .await
    .expect("seed current assignment");
}

fn delegated_headers(root: &Wallet, session: &Wallet, metadata: &str) -> HeaderMap {
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    let signed_fetch_payload = build_payload("post", ISLAND_REFRESH_PATH, &timestamp, metadata);
    let expiration = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: {expiration}",
        session.address()
    );
    let links = [
        json!({ "type": "SIGNER", "payload": root.address(), "signature": "" }),
        json!({
            "type": "ECDSA_EPHEMERAL",
            "payload": ephemeral_payload,
            "signature": root.sign_message(ephemeral_payload.as_bytes()).unwrap()
        }),
        json!({
            "type": "ECDSA_SIGNED_ENTITY",
            "payload": signed_fetch_payload,
            "signature": session.sign_message(signed_fetch_payload.as_bytes()).unwrap()
        }),
    ];
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTH_TIMESTAMP_HEADER,
        HeaderValue::from_str(&timestamp).unwrap(),
    );
    headers.insert(
        AUTH_METADATA_HEADER,
        HeaderValue::from_str(metadata).unwrap(),
    );
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static(ISLAND_REFRESH_CONTENT_TYPE),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(ISLAND_REFRESH_CONTENT_TYPE),
    );
    for (index, link) in links.into_iter().enumerate() {
        headers.insert(
            HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{index}").as_bytes())
                .unwrap(),
            HeaderValue::from_str(&link.to_string()).unwrap(),
        );
    }
    headers
}

fn guest_headers(wallet: &Wallet) -> HeaderMap {
    let mut headers = support::signed_headers(wallet, "post", ISLAND_REFRESH_PATH);
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static(ISLAND_REFRESH_CONTENT_TYPE),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(ISLAND_REFRESH_CONTENT_TYPE),
    );
    headers
}

async fn call(
    state: catalyrst_comms::AppState,
    reader: Arc<dyn RealmAssignmentReader>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match refresh(State(state), Extension(reader), headers, body).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

fn decode_claims(adapter: &str) -> serde_json::Value {
    let token = adapter
        .split("access_token=")
        .nth(1)
        .expect("adapter access token");
    let payload = token.split('.').nth(1).expect("JWT payload");
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).expect("base64url payload"))
        .expect("JWT JSON payload")
}

#[tokio::test]
async fn delegated_current_owner_refreshes_the_same_fence_without_revision_mutation() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    seed_assignment(
        &fixture.pool,
        &root.address(),
        &session.address(),
        "island-current",
        9,
    )
    .await;
    let before: (String, i64, String, i64, i64, serde_json::Value) = sqlx::query_as(
        "SELECT authority_incarnation, assignment_revision, owner_session, owner_epoch,
                fencing_token, assignment_json
         FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(root.address())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    let response = call(
        state,
        fixture.reader().await,
        delegated_headers(&root, &session, "{}"),
        Bytes::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        ISLAND_REFRESH_CONTENT_TYPE
    );
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let response = IslandRefreshResponse::decode(
        to_bytes(response.into_body(), 16 * 1024)
            .await
            .unwrap()
            .as_ref(),
    )
    .expect("decode refresh response");
    assert_eq!(
        response.expires_in_seconds,
        ISLAND_REFRESH_TTL_SECONDS as u32
    );
    let claims = decode_claims(&response.adapter);
    assert_eq!(claims["sub"], root.address());
    assert_eq!(claims["video"]["room"], "island-current");
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["nbf"].as_u64().unwrap(),
        60
    );
    let metadata: serde_json::Value =
        serde_json::from_str(claims["metadata"].as_str().unwrap()).unwrap();
    assert_eq!(metadata["catalyrstIsland"]["version"], 2);
    assert_eq!(metadata["catalyrstIsland"]["wallet"], root.address());
    assert_eq!(metadata["catalyrstIsland"]["session"], session.address());
    assert_eq!(metadata["catalyrstIsland"]["audience"], AUDIENCE);
    assert_eq!(metadata["catalyrstIsland"]["lane"], "realm");
    assert_eq!(metadata["catalyrstIsland"]["ownerEpoch"], 7);
    assert_eq!(metadata["catalyrstIsland"]["assignmentRevision"], 9);
    assert_eq!(metadata["catalyrstIsland"]["fencingToken"], 11);
    let after: (String, i64, String, i64, i64, serde_json::Value) = sqlx::query_as(
        "SELECT authority_incarnation, assignment_revision, owner_session, owner_epoch,
                fencing_token, assignment_json
         FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(root.address())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(after, before);

    fixture.finish().await;
}

#[tokio::test]
async fn guest_root_owner_and_wrong_sessions_follow_the_current_row() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    seed_assignment(
        &fixture.pool,
        &root.address(),
        &session.address(),
        "island-current",
        9,
    )
    .await;
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    assert_eq!(
        call(
            state.clone(),
            fixture.reader().await,
            guest_headers(&root),
            Bytes::new(),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    sqlx::query(
        "UPDATE archipelago_v4_assignments
         SET owner_session = $3, updated_at = now()
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(root.address())
    .bind(root.address())
    .execute(&fixture.pool)
    .await
    .unwrap();
    let response = call(
        state.clone(),
        fixture.reader().await,
        guest_headers(&root),
        Bytes::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = IslandRefreshResponse::decode(
        to_bytes(response.into_body(), 16 * 1024)
            .await
            .unwrap()
            .as_ref(),
    )
    .expect("decode guest refresh response");
    let claims = decode_claims(&response.adapter);
    let metadata: serde_json::Value =
        serde_json::from_str(claims["metadata"].as_str().unwrap()).unwrap();
    assert_eq!(metadata["catalyrstIsland"]["session"], root.address());

    let stale =
        Wallet::from_hex("0x0dbbe8e4f543c7603571f52217cb251c8b04cd3ca70a6b213c11a45dbf346a82")
            .unwrap();
    assert_eq!(
        call(
            state,
            fixture.reader().await,
            delegated_headers(&root, &stale, "{}"),
            Bytes::new(),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    fixture.finish().await;
}

#[tokio::test]
async fn lapsed_same_session_owner_cannot_refresh_old_credentials() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    seed_assignment(
        &fixture.pool,
        &root.address(),
        &session.address(),
        "island-current",
        9,
    )
    .await;
    sqlx::query(
        "UPDATE archipelago_v4_assignments
         SET updated_at = now() - interval '91 seconds'
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(root.address())
    .execute(&fixture.pool)
    .await
    .unwrap();
    let before: (i64, serde_json::Value) = sqlx::query_as(
        "SELECT assignment_revision, assignment_json
         FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(root.address())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    let response = call(
        state,
        fixture.reader().await,
        delegated_headers(&root, &session, "{}"),
        Bytes::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());
    let after: (i64, serde_json::Value) = sqlx::query_as(
        "SELECT assignment_revision, assignment_json
         FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(root.address())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(after, before);

    fixture.finish().await;
}

#[tokio::test]
async fn supplied_device_cannot_hide_a_recorded_device_ban() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    seed_assignment(
        &fixture.pool,
        &root.address(),
        &session.address(),
        "island-current",
        9,
    )
    .await;
    sqlx::query(
        "INSERT INTO player_connection_info (address, device_id) VALUES ($1, 'banned-device')",
    )
    .bind(root.address())
    .execute(&fixture.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_bans (banned_address, banned_device_id)
         VALUES ('0x00000000000000000000000000000000000000ff', 'banned-device')",
    )
    .execute(&fixture.pool)
    .await
    .unwrap();
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    let response = call(
        state,
        fixture.reader().await,
        delegated_headers(&root, &session, r#"{"deviceIdentifier":"clean-device"}"#),
        Bytes::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());

    fixture.finish().await;
}

struct SequenceReader {
    snapshots: Mutex<VecDeque<RealmAssignmentSnapshot>>,
}

struct SlowReader;

#[async_trait]
impl RealmAssignmentReader for SlowReader {
    async fn current_realm_assignment(
        &self,
        _wallet: &str,
        _session: &str,
    ) -> Result<RealmAssignmentSnapshot, FenceError> {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        Err(FenceError::Unavailable)
    }
}

#[async_trait]
impl RealmAssignmentReader for SequenceReader {
    async fn current_realm_assignment(
        &self,
        _wallet: &str,
        _session: &str,
    ) -> Result<RealmAssignmentSnapshot, FenceError> {
        self.snapshots
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(FenceError::Unavailable)
    }
}

#[tokio::test]
async fn room_or_generation_change_during_mint_returns_no_credential() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    let before = RealmAssignmentSnapshot {
        audience: AUDIENCE.into(),
        wallet: root.address(),
        lane: "realm".into(),
        authority_incarnation: "authority-refresh-test".into(),
        owner_session: session.address(),
        owner_epoch: 7,
        assignment_revision: 9,
        fencing_token: 11,
        island_id: "island-old".into(),
    };
    let mut after = before.clone();
    after.assignment_revision += 1;
    after.island_id = "island-new".into();
    let reader: Arc<dyn RealmAssignmentReader> = Arc::new(SequenceReader {
        snapshots: Mutex::new(VecDeque::from([before, after])),
    });
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    let response = call(
        state,
        reader,
        delegated_headers(&root, &session, "{}"),
        Bytes::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        ISLAND_REFRESH_CONTENT_TYPE
    );
    assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());

    fixture.finish().await;
}

#[tokio::test]
async fn protobuf_headers_and_empty_body_are_mandatory() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    let reader = fixture.reader().await;

    let mut wrong_content = delegated_headers(&root, &session, "{}");
    wrong_content.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    assert_eq!(
        call(state.clone(), reader.clone(), wrong_content, Bytes::new())
            .await
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let mut wrong_accept = delegated_headers(&root, &session, "{}");
    wrong_accept.insert(ACCEPT, HeaderValue::from_static("application/json"));
    assert_eq!(
        call(state.clone(), reader.clone(), wrong_accept, Bytes::new())
            .await
            .status(),
        StatusCode::NOT_ACCEPTABLE
    );
    assert_eq!(
        call(
            state,
            reader,
            delegated_headers(&root, &session, "{}"),
            Bytes::from_static(b"not empty"),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    fixture.finish().await;
}

#[tokio::test]
async fn slow_authority_work_is_cut_off_by_the_request_deadline() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let state = support::test_state(fixture.pool.clone(), None, None, "unused");
    let root = Wallet::from_hex(ROOT_KEY).unwrap();
    let session = Wallet::from_hex(SESSION_KEY).unwrap();
    let started = tokio::time::Instant::now();
    let response = call(
        state,
        Arc::new(SlowReader),
        delegated_headers(&root, &session, "{}"),
        Bytes::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(started.elapsed() < std::time::Duration::from_millis(2_500));
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());

    fixture.finish().await;
}
