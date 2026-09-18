mod support;

use std::{sync::Arc, time::Duration};

use axum::http::header::{ACCEPT, CONTENT_TYPE};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use catalyrst_comms::{
    assignment_fence::{PgAssignmentFence, RealmAssignmentReader},
    handlers::island_refresh::{
        IslandRefreshResponse, ISLAND_REFRESH_CONTENT_TYPE, ISLAND_REFRESH_PATH,
        ISLAND_REFRESH_TTL_SECONDS,
    },
    relay_authority::{
        wire::{self, authority_request, authority_response},
        DenialReason, ENDPOINT,
    },
};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_crypto::Wallet;
use catalyrst_livekit::sign_hs256;
use catalyrst_pulse::application_relay::{
    auth::{address_bytes, RoomClaims, RoomGrant, VerifiedRoom, PROOF_DOMAIN},
    http::HttpAuthority,
    RoomAuthority,
};
use hmac::{Hmac, KeyInit, Mac};
use prost::Message;
use sha2::{Digest, Sha256};

const AUDIENCE: &str = "relay-authority-http";
const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION: &str = "0x2222222222222222222222222222222222222222";
const LIVEKIT_KEY: &str = "relay-http-key";
const LIVEKIT_SECRET: &str = "01234567890123456789012345678901";
const SERVICE_KEY: [u8; 32] = [9; 32];
const ROOT_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const PUBLIC_REFRESH_PATH: &str = "/comms/island-refresh";

struct Fixture {
    db: ScratchSchema,
}

impl Fixture {
    async fn new() -> Option<Self> {
        let db = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "relay_authority_http").await?;
        for migration in [
            include_str!("../migrations/0001_comms.sql"),
            include_str!("../migrations/0002_user_moderation.sql"),
            include_str!("../migrations/0006_player_connection_and_device_bans.sql"),
        ] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(migration))
                .execute(&db.pool)
                .await
                .unwrap();
        }
        sqlx::raw_sql(
            "CREATE TABLE archipelago_v4_assignments (
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
            )",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        let fixture = Self { db };
        fixture.seed(4, 5, 6).await;
        Some(fixture)
    }

    async fn seed(&self, epoch: i64, revision: i64, fence: i64) {
        self.seed_for(WALLET, SESSION, epoch, revision, fence).await;
    }

    async fn seed_for(&self, wallet: &str, session: &str, epoch: i64, revision: i64, fence: i64) {
        sqlx::query(
            "INSERT INTO archipelago_v4_assignments (
                deployment_audience, owner_address, lane_key, authority_incarnation,
                assignment_revision, owner_session, owner_epoch, fencing_token, assignment_json
             ) VALUES ($1, $2, 'realm', 'relay-authority', $3, $4, $5, $6, $7)
             ON CONFLICT (deployment_audience, owner_address, lane_key) DO UPDATE SET
                assignment_revision = EXCLUDED.assignment_revision,
                owner_session = EXCLUDED.owner_session,
                owner_epoch = EXCLUDED.owner_epoch,
                fencing_token = EXCLUDED.fencing_token,
                assignment_json = EXCLUDED.assignment_json",
        )
        .bind(AUDIENCE)
        .bind(wallet)
        .bind(revision)
        .bind(session)
        .bind(epoch)
        .bind(fence)
        .bind(serde_json::json!({
            "islandId": "island-123",
            "connectionString": "livekit:opaque",
            "tokenExpiresAtUnix": 1,
        }))
        .execute(&self.db.pool)
        .await
        .unwrap();
    }

    async fn finish(self) {
        self.db.drop().await;
    }
}

fn credential(expires_at: u64, nonce: [u8; 32]) -> VerifiedRoom {
    let metadata = serde_json::json!({"catalyrstIsland": {
        "version": 2,
        "wallet": WALLET,
        "session": SESSION,
        "audience": AUDIENCE,
        "lane": "realm",
        "ownerEpoch": 4,
        "assignmentRevision": 5,
        "fencingToken": 6,
    }})
    .to_string();
    let claims = RoomClaims {
        iss: LIVEKIT_KEY.into(),
        sub: WALLET.into(),
        exp: expires_at,
        nbf: 0,
        video: RoomGrant {
            room: "island-123".into(),
            room_join: true,
            can_publish_data: true,
        },
        metadata,
    };
    let header_payload = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256"}"#),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    );
    let mut jwt = Hmac::<Sha256>::new_from_slice(LIVEKIT_SECRET.as_bytes()).unwrap();
    jwt.update(header_payload.as_bytes());
    let mut proof = Hmac::<Sha256>::new_from_slice(&jwt.finalize().into_bytes()).unwrap();
    proof.update(PROOF_DOMAIN);
    proof.update(&nonce);
    proof.update(&address_bytes(WALLET).unwrap());
    proof.update(&address_bytes(SESSION).unwrap());
    proof.update(&Sha256::digest(header_payload.as_bytes()));
    VerifiedRoom {
        claims,
        wallet: WALLET.into(),
        session: SESSION.into(),
        is_guest: false,
        header_payload,
        nonce,
        proof: proof.finalize().into_bytes().to_vec(),
    }
}

async fn start(fixture: &Fixture) -> (String, tokio::task::JoinHandle<()>) {
    let mut state = support::test_state(fixture.db.pool.clone(), None, None, "unused");
    let state_inner = Arc::get_mut(&mut state).expect("fixture state is unshared");
    state_inner.livekit_api_url = "http://127.0.0.1:1".into();
    state_inner.livekit_api_key = LIVEKIT_KEY.into();
    state_inner.livekit_api_secret = LIVEKIT_SECRET.into();
    let assignments: Arc<dyn RealmAssignmentReader> = Arc::new(
        PgAssignmentFence::connect(&fixture.db.url(), AUDIENCE.into())
            .await
            .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let internal = catalyrst_comms::connection_router(
            state.clone(),
            Some(assignments.clone()),
            Some(SERVICE_KEY),
        )
        .unwrap();
        let public =
            catalyrst_comms::connection_router(state.clone(), Some(assignments), None).unwrap();
        let app = internal.nest("/comms", public).with_state(state);
        axum::serve(listener, app).await.unwrap()
    });
    (base_url, task)
}

fn authority_endpoint(base_url: &str) -> String {
    format!("{base_url}{ENDPOINT}")
}

async fn refresh_request(base_url: &str, wallet: &Wallet, signed_path: &str) -> reqwest::Response {
    let mut headers = support::signed_headers(wallet, "post", signed_path);
    headers.insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static(ISLAND_REFRESH_CONTENT_TYPE),
    );
    headers.insert(
        ACCEPT,
        axum::http::HeaderValue::from_static(ISLAND_REFRESH_CONTENT_TYPE),
    );
    headers.insert(
        "x-original-path",
        axum::http::HeaderValue::from_static(PUBLIC_REFRESH_PATH),
    );
    reqwest::Client::new()
        .post(format!("{base_url}{PUBLIC_REFRESH_PATH}"))
        .headers(headers)
        .body(Vec::new())
        .send()
        .await
        .unwrap()
}

async fn refresh_token(base_url: &str, wallet: &Wallet) -> String {
    let response = refresh_request(base_url, wallet, ISLAND_REFRESH_PATH).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let refresh = IslandRefreshResponse::decode(response.bytes().await.unwrap().as_ref()).unwrap();
    assert_eq!(
        refresh.expires_in_seconds,
        ISLAND_REFRESH_TTL_SECONDS as u32
    );
    refresh
        .adapter
        .split("access_token=")
        .nth(1)
        .expect("refresh adapter access token")
        .to_owned()
}

fn credential_from_token(
    token: &str,
    wallet: &str,
    session: &str,
    nonce: [u8; 32],
) -> VerifiedRoom {
    let mut parts = token.split('.');
    let header = parts.next().expect("JWT header");
    let payload = parts.next().expect("JWT payload");
    let signature = parts.next().expect("JWT signature");
    assert!(parts.next().is_none(), "JWT has exactly three parts");
    let header_payload = format!("{header}.{payload}");
    let claims: RoomClaims =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
    let signature = URL_SAFE_NO_PAD.decode(signature).unwrap();
    let mut proof = Hmac::<Sha256>::new_from_slice(&signature).unwrap();
    proof.update(PROOF_DOMAIN);
    proof.update(&nonce);
    proof.update(&address_bytes(wallet).unwrap());
    proof.update(&address_bytes(session).unwrap());
    proof.update(&Sha256::digest(header_payload.as_bytes()));
    VerifiedRoom {
        claims,
        wallet: wallet.to_ascii_lowercase(),
        session: session.to_ascii_lowercase(),
        is_guest: true,
        header_payload,
        nonce,
        proof: proof.finalize().into_bytes().to_vec(),
    }
}

async fn tuple(
    fixture: &Fixture,
    wallet: &str,
) -> (String, i64, String, i64, i64, serde_json::Value) {
    sqlx::query_as(
        "SELECT authority_incarnation, assignment_revision, owner_session, owner_epoch,
                fencing_token, assignment_json
         FROM archipelago_v4_assignments
         WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
    )
    .bind(AUDIENCE)
    .bind(wallet)
    .fetch_one(&fixture.db.pool)
    .await
    .unwrap()
}

async fn raw(
    endpoint: &str,
    nonce_byte: u8,
    operation: authority_request::Operation,
) -> wire::AuthorityResponse {
    let body = wire::AuthorityRequest {
        operation: Some(operation),
    }
    .encode_to_vec();
    let nonce = URL_SAFE_NO_PAD.encode([nonce_byte; 16]);
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let preimage = format!(
        "POST\n{ENDPOINT}\n{timestamp}\n{nonce}\n{}",
        hex::encode(Sha256::digest(&body))
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(&SERVICE_KEY).unwrap();
    mac.update(preimage.as_bytes());
    let response = reqwest::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/x-protobuf")
        .header("Accept", "application/x-protobuf")
        .header("X-Pulse-Authority-Timestamp", timestamp)
        .header("X-Pulse-Authority-Nonce", nonce)
        .header(
            "X-Pulse-Authority-Signature",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()),
        )
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    wire::AuthorityResponse::decode(response.bytes().await.unwrap().as_ref()).unwrap()
}

fn denied(response: wire::AuthorityResponse) -> DenialReason {
    let Some(authority_response::Result::Denied(denied)) = response.result else {
        panic!("denial expected")
    };
    DenialReason::try_from(denied.reason).unwrap()
}

#[tokio::test]
async fn pulse_http_authority_composes_with_relay_router_and_current_fence() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let (base_url, server) = start(&fixture).await;
    let endpoint = authority_endpoint(&base_url);
    let encoded_key = URL_SAFE_NO_PAD.encode(SERVICE_KEY);
    let client = HttpAuthority::new(&endpoint, &encoded_key, true).unwrap();
    let valid = credential(chrono::Utc::now().timestamp() as u64 + 30, [1; 32]);
    assert_eq!(
        client.authorize(valid.clone(), false).await,
        Ok(Duration::from_secs(2))
    );
    assert_eq!(
        client.authorize(valid.clone(), true).await,
        Ok(Duration::from_secs(2))
    );

    let manual = credential(chrono::Utc::now().timestamp() as u64 + 30, [2; 32]);
    let join = wire::Join {
        pulse_instance: vec![3; 16],
        connection_generation: vec![4; 16],
        wallet: address_bytes(&manual.wallet).unwrap().to_vec(),
        session: address_bytes(&manual.session).unwrap().to_vec(),
        header_payload: manual.header_payload.clone(),
        nonce: manual.nonce.to_vec(),
        proof: manual.proof.clone(),
    };
    let response = raw(&endpoint, 1, authority_request::Operation::Join(join)).await;
    let Some(authority_response::Result::Lease(lease)) = response.result else {
        panic!("lease expected")
    };
    assert_eq!(lease.lease_id.len(), 16);
    assert_eq!(lease.authority_incarnation.len(), 16);
    assert_eq!(lease.ttl_ms, 2_000);
    assert_eq!(
        denied(
            raw(
                &endpoint,
                2,
                authority_request::Operation::Renew(wire::Renew {
                    pulse_instance: vec![3; 16],
                    connection_generation: vec![5; 16],
                    lease_id: lease.lease_id.clone(),
                    authority_incarnation: lease.authority_incarnation.clone(),
                }),
            )
            .await,
        ),
        DenialReason::Invalid
    );
    assert_eq!(
        denied(
            raw(
                &endpoint,
                3,
                authority_request::Operation::Renew(wire::Renew {
                    pulse_instance: vec![3; 16],
                    connection_generation: vec![4; 16],
                    lease_id: lease.lease_id.clone(),
                    authority_incarnation: vec![0; 16],
                }),
            )
            .await,
        ),
        DenialReason::Expired
    );

    fixture.seed(5, 6, 7).await;
    assert!(client.authorize(valid.clone(), true).await.is_err());
    assert_eq!(
        denied(
            raw(
                &endpoint,
                4,
                authority_request::Operation::Renew(wire::Renew {
                    pulse_instance: vec![3; 16],
                    connection_generation: vec![4; 16],
                    lease_id: lease.lease_id.clone(),
                    authority_incarnation: lease.authority_incarnation.clone(),
                }),
            )
            .await,
        ),
        DenialReason::NotAuthorized
    );

    fixture.seed(4, 5, 6).await;
    let banned = HttpAuthority::new(&endpoint, &encoded_key, true).unwrap();
    assert!(banned
        .authorize(
            credential(chrono::Utc::now().timestamp() as u64 + 30, [3; 32]),
            false
        )
        .await
        .is_ok());
    sqlx::query(
        "INSERT INTO user_bans(banned_address,banned_by,reason) VALUES ($1,'admin','test')",
    )
    .bind(WALLET)
    .execute(&fixture.db.pool)
    .await
    .unwrap();
    assert!(banned
        .authorize(
            credential(chrono::Utc::now().timestamp() as u64 + 30, [3; 32]),
            true
        )
        .await
        .is_err());
    sqlx::query("DELETE FROM user_bans")
        .execute(&fixture.db.pool)
        .await
        .unwrap();

    let expires_at = chrono::Utc::now().timestamp() as u64 + 1;
    let expiring = credential(expires_at, [4; 32]);
    let live = HttpAuthority::new(&endpoint, &encoded_key, true).unwrap();
    assert!(live.authorize(expiring.clone(), false).await.is_ok());
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    assert!(live.authorize(expiring, true).await.is_ok());
    let fresh = HttpAuthority::new(&endpoint, &encoded_key, true).unwrap();
    assert!(fresh
        .authorize(credential(expires_at, [5; 32]), false)
        .await
        .is_err());

    server.abort();
    let _ = server.await;
    let unavailable = HttpAuthority::new(&endpoint, &encoded_key, true).unwrap();
    assert!(unavailable
        .authorize(
            credential(chrono::Utc::now().timestamp() as u64 + 30, [6; 32]),
            false
        )
        .await
        .is_err());
    drop((unavailable, fresh, live, banned, client));
    fixture.finish().await;
}

#[tokio::test]
async fn production_refresh_token_composes_with_pulse_http_authority() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let owner = Wallet::from_hex(ROOT_KEY).unwrap();
    fixture
        .seed_for(&owner.address(), &owner.address(), 4, 5, 6)
        .await;
    let before = tuple(&fixture, &owner.address()).await;
    let (base_url, server) = start(&fixture).await;
    let endpoint = authority_endpoint(&base_url);
    assert_eq!(
        refresh_request(&base_url, &owner, PUBLIC_REFRESH_PATH)
            .await
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let token = refresh_token(&base_url, &owner).await;
    let room = credential_from_token(&token, &owner.address(), &owner.address(), [7; 32]);
    assert_eq!(room.claims.video.room, "island-123");
    let client = HttpAuthority::new(&endpoint, &URL_SAFE_NO_PAD.encode(SERVICE_KEY), true).unwrap();
    assert_eq!(
        client.authorize(room.clone(), false).await,
        Ok(Duration::from_secs(2))
    );
    assert_eq!(
        client.authorize(room.clone(), true).await,
        Ok(Duration::from_secs(2))
    );
    assert_eq!(tuple(&fixture, &owner.address()).await, before);

    let mut expired_claims = room.claims.clone();
    expired_claims.exp = chrono::Utc::now().timestamp() as u64 - 1;
    let expired_token = sign_hs256(
        LIVEKIT_SECRET,
        br#"{"alg":"HS256","typ":"JWT"}"#,
        &serde_json::to_vec(&expired_claims).unwrap(),
    )
    .unwrap();
    let expired =
        credential_from_token(&expired_token, &owner.address(), &owner.address(), [8; 32]);
    let fresh = HttpAuthority::new(&endpoint, &URL_SAFE_NO_PAD.encode(SERVICE_KEY), true).unwrap();
    assert!(fresh.authorize(expired, false).await.is_err());

    server.abort();
    let _ = server.await;
    drop((fresh, client));
    fixture.finish().await;
}
