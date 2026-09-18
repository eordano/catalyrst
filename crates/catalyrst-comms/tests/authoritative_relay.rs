mod support;

use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::get, Json, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use catalyrst_comms::{
    handlers::scene_adapter::{get_server_scene_adapter, SceneAdapterRequest},
    relay_authority::ENDPOINT,
    AppState,
};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_crypto::Wallet;
use catalyrst_livekit::sign_hs256;
use catalyrst_pulse::application_relay::{
    auth::{address_bytes, RoomClaims, VerifiedRoom, PROOF_DOMAIN},
    http::HttpAuthority,
    RoomAuthority,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const AUTHORITATIVE_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const GUEST_KEY: &str = "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba";
const LIVEKIT_KEY: &str = "authoritative-relay-key";
const LIVEKIT_SECRET: &str = "authoritative-relay-signing-secret";
const SERVICE_KEY: [u8; 32] = [17; 32];

async fn database() -> Option<ScratchSchema> {
    let db = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "authoritative_relay").await?;
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
    Some(db)
}

fn state(db: &ScratchSchema, authority: Option<String>, world_url: &str) -> AppState {
    let mut state = support::test_state(db.pool.clone(), None, None, "unused");
    let inner = Arc::get_mut(&mut state).unwrap();
    inner.authoritative_server_address = authority;
    inner.livekit_api_key = LIVEKIT_KEY.into();
    inner.livekit_api_secret = LIVEKIT_SECRET.into();
    inner.world_content_url = world_url.into();
    state
}

async fn mint(state: AppState, realm: &str) -> String {
    let signer = Wallet::from_hex(AUTHORITATIVE_KEY).unwrap();
    let headers = support::signed_headers(&signer, "post", "/get-server-scene-adapter");
    let Json(response) = get_server_scene_adapter(
        State(state),
        headers,
        Json(SceneAdapterRequest {
            scene_id: Some("scene-a".into()),
            realm_name: Some(realm.into()),
            parcel: None,
        }),
    )
    .await
    .expect("verified authoritative signer mints a scene adapter");
    response
        .adapter
        .split("access_token=")
        .nth(1)
        .unwrap()
        .into()
}

fn credential(token: &str, nonce_byte: u8) -> VerifiedRoom {
    let guest = Wallet::from_hex(GUEST_KEY)
        .unwrap()
        .address()
        .to_lowercase();
    let (header_payload, signature) = token.rsplit_once('.').unwrap();
    let (_, payload) = header_payload.split_once('.').unwrap();
    let claims: RoomClaims =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
    let nonce = [nonce_byte; 32];
    let mut proof =
        Hmac::<Sha256>::new_from_slice(&URL_SAFE_NO_PAD.decode(signature).unwrap()).unwrap();
    proof.update(PROOF_DOMAIN);
    proof.update(&nonce);
    proof.update(&address_bytes(&guest).unwrap());
    proof.update(&address_bytes(&guest).unwrap());
    proof.update(&Sha256::digest(header_payload.as_bytes()));
    VerifiedRoom {
        claims,
        wallet: guest.clone(),
        session: guest,
        is_guest: true,
        header_payload: header_payload.into(),
        nonce,
        proof: proof.finalize().into_bytes().to_vec(),
    }
}

async fn start(state: AppState) -> (HttpAuthority, tokio::task::JoinHandle<()>) {
    let app = catalyrst_comms::connection_router(state.clone(), None, Some(SERVICE_KEY))
        .unwrap()
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}{ENDPOINT}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = HttpAuthority::new(&endpoint, &URL_SAFE_NO_PAD.encode(SERVICE_KEY), true).unwrap();
    (client, server)
}

async fn stop(server: tokio::task::JoinHandle<()>) {
    server.abort();
    let _ = server.await;
}

fn changed_token(token: &str, change: impl FnOnce(&mut Value)) -> String {
    let mut payload: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    change(&mut payload);
    sign_hs256(
        LIVEKIT_SECRET,
        br#"{"alg":"HS256"}"#,
        &serde_json::to_vec(&payload).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn signed_authoritative_mint_accepts_a_distinct_guest_and_rechecks_both_wallet_bans() {
    let Some(db) = database().await else { return };
    let authority = Wallet::from_hex(AUTHORITATIVE_KEY)
        .unwrap()
        .address()
        .to_lowercase();
    let state = state(&db, Some(authority.clone()), "http://127.0.0.1:1");
    let token = mint(state.clone(), "main").await;
    let room = credential(&token, 1);
    assert_ne!(room.wallet, authority);
    assert!(room.is_guest);
    assert_eq!(room.claims.sub, "authoritative-server");
    assert_eq!(room.claims.video.room, "scene:main:scene-a");
    assert_eq!(
        serde_json::from_str::<Value>(&room.claims.metadata).unwrap(),
        json!({"catalyrstAuthoritativeServer":{"wallet":authority}})
    );
    let (client, server) = start(state).await;
    assert!(client.authorize(room.clone(), false).await.is_ok());
    assert!(client.authorize(room.clone(), true).await.is_ok());

    for (index, banned_wallet) in [&authority, &room.wallet].into_iter().enumerate() {
        let current = credential(&token, index as u8 + 2);
        assert!(client.authorize(current.clone(), false).await.is_ok());
        sqlx::query(
            "INSERT INTO user_bans(banned_address,banned_by,reason) VALUES ($1,'admin','test')",
        )
        .bind(banned_wallet)
        .execute(&db.pool)
        .await
        .unwrap();
        assert!(client.authorize(current.clone(), true).await.is_err());
        sqlx::query("DELETE FROM user_bans")
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(
            client.authorize(current, true).await.is_err(),
            "denied lease cannot revive"
        );
    }
    let current = credential(&token, 4);
    assert!(client.authorize(current.clone(), false).await.is_ok());
    sqlx::query(
        "INSERT INTO scene_bans(place_id,banned_address,banned_by) VALUES ('scene-a',$1,'admin')",
    )
    .bind(&authority)
    .execute(&db.pool)
    .await
    .unwrap();
    assert!(client.authorize(current, true).await.is_err());
    stop(server).await;
    db.drop().await;
}

#[tokio::test]
async fn authoritative_metadata_is_signed_required_current_and_never_grants_island_scope() {
    let Some(db) = database().await else { return };
    let authority = Wallet::from_hex(AUTHORITATIVE_KEY)
        .unwrap()
        .address()
        .to_lowercase();
    let initial = state(&db, Some(authority.clone()), "http://127.0.0.1:1");
    let token = mint(initial.clone(), "main").await;
    let (client, server) = start(initial).await;
    let guest = credential(&token, 1).wallet;
    for (index, metadata) in [
        String::new(),
        json!({"catalyrstAuthoritativeServer":{"wallet":guest}}).to_string(),
        json!({"catalyrstAuthoritativeServer":{"wallet":"invalid"}}).to_string(),
    ]
    .into_iter()
    .enumerate()
    {
        let invalid = changed_token(&token, |claims| claims["metadata"] = metadata.into());
        assert!(client
            .authorize(credential(&invalid, index as u8 + 1), false)
            .await
            .is_err());
    }
    let island = changed_token(&token, |claims| {
        claims["video"]["room"] = "island-123".into()
    });
    assert!(client
        .authorize(credential(&island, 4), false)
        .await
        .is_err());
    let mut tampered = credential(&token, 5);
    tampered.proof[0] ^= 1;
    assert!(client.authorize(tampered, false).await.is_err());
    let mut rebound = credential(&token, 6);
    rebound.wallet = authority.clone();
    assert!(client.authorize(rebound, false).await.is_err());
    assert!(client.authorize(credential(&token, 7), false).await.is_ok());
    stop(server).await;
    for configured in [None, Some(guest)] {
        let (client, server) = start(state(&db, configured, "http://127.0.0.1:1")).await;
        assert!(client
            .authorize(credential(&token, 8), false)
            .await
            .is_err());
        stop(server).await;
    }
    db.drop().await;
}

#[tokio::test]
async fn authoritative_world_renewal_uses_the_minting_wallet_and_current_world_permissions() {
    let Some(db) = database().await else { return };
    let authority = Wallet::from_hex(AUTHORITATIVE_KEY)
        .unwrap()
        .address()
        .to_lowercase();
    let permissions = Arc::new(Mutex::new(json!({"permissions":{"access":{
        "type":"allow-list", "wallets":[authority]
    }}})));
    let current = permissions.clone();
    let app = Router::new().route(
        "/world/example.eth/permissions",
        get(move || {
            let body = current.lock().unwrap().clone();
            async { Json(body) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let world_url = format!("http://{}", listener.local_addr().unwrap());
    let world_server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let state = state(&db, Some(authority), &world_url);
    let token = mint(state.clone(), "example.eth").await;
    let room = credential(&token, 1);
    assert_eq!(room.claims.video.room, "world-example.eth-scene-a");
    let (client, server) = start(state).await;
    assert!(client.authorize(room.clone(), false).await.is_ok());
    *permissions.lock().unwrap() = json!({"permissions":{"access":{
        "type":"allow-list", "wallets":[room.wallet]
    }}});
    assert!(client.authorize(room.clone(), true).await.is_err());
    assert!(client
        .authorize(credential(&token, 2), false)
        .await
        .is_err());
    stop(server).await;
    stop(world_server).await;
    db.drop().await;
}
