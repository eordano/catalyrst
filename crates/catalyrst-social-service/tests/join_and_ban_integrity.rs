use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use rand::Rng;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};
use catalyrst_social_service::rest::content_store::ContentStore;
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::handlers::client;
use catalyrst_social_service::rest::handlers::writes;
use catalyrst_social_service::rest::ports::bans::BansComponent;
use catalyrst_social_service::rest::ports::communities::CommunitiesComponent;
use catalyrst_social_service::rest::ports::invites::InvitesComponent;
use catalyrst_social_service::rest::ports::members::MembersComponent;
use catalyrst_social_service::rest::ports::moderation::ModerationComponent;
use catalyrst_social_service::rest::ports::places::PlacesComponent;
use catalyrst_social_service::rest::ports::places_api::PlacesApiClient;
use catalyrst_social_service::rest::ports::posts::PostsComponent;
use catalyrst_social_service::rest::ports::profiles::ProfilesComponent;
use catalyrst_social_service::rest::ports::requests::RequestsComponent;
use catalyrst_social_service::rest::ports::voice::VoiceComponent;
use catalyrst_social_service::rest::{AppState, AppStateInner};

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_SOCIAL_SERVICE_TEST_PG")
        .ok()
        .or_else(|| Some("postgres://postgres:postgres@127.0.0.1:5432/communities".into()))
}

fn unique_schema() -> String {
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    format!("test_ban_{}", hex::encode(b))
}

fn unique_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rnd = [0u8; 8];
    rand::rng().fill_bytes(&mut rnd);
    p.push(format!("cmm-ban-{}-{}", tag, hex::encode(rnd)));
    p
}

async fn setup_db() -> Option<(PgPool, String, String)> {
    let url = pg_url()?;
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
    let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&suffixed)
        .await
        .ok()?;

    apply_migration(&pool, include_str!("../migrations/0001_initial.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0002_federation.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0003_voice_moderators.sql"),
    )
    .await;
    apply_migration(&pool, include_str!("../migrations/0004_thumbnail_hash.sql")).await;
    apply_migration(&pool, include_str!("../migrations/0005_suspension.sql")).await;
    apply_migration(
        &pool,
        include_str!("../migrations/0006_role_check_reconcile.sql"),
    )
    .await;

    Some((pool, schema, url))
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_block_comments(sql);
    let mut buf = String::new();
    let mut in_func = false;
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
        if trimmed.contains("$$ LANGUAGE plpgsql;") {
            in_func = false;
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
            continue;
        }
        if trimmed.contains("CREATE OR REPLACE FUNCTION") || trimmed.contains("CREATE FUNCTION") {
            in_func = true;
        }
        if !in_func && trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
                .execute(pool)
                .await
                .unwrap_or_else(|_| panic!("{}", buf.clone()));
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        sqlx::query(sqlx::AssertSqlSafe(buf.as_str()))
            .execute(pool)
            .await
            .expect("trailing sql");
    }
}

fn strip_block_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
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

async fn build_state(pool: &PgPool) -> (AppState, PathBuf) {
    let content_dir = unique_dir("state");
    let content_store = Arc::new(ContentStore::new(&content_dir));
    content_store.init().await.expect("content store init");
    let replay = Replay::new(pool.clone()).await.expect("replay init");

    let state = Arc::new(AppStateInner {
        admin_token: None,
        bans: BansComponent::new(pool.clone()),
        communities: CommunitiesComponent::new(pool.clone()),
        invites: InvitesComponent::new(pool.clone()),
        members: MembersComponent::new(pool.clone()),
        moderation: ModerationComponent::new(pool.clone()),
        places: PlacesComponent::new(pool.clone()),
        places_api: PlacesApiClient::new(None),
        posts: PostsComponent::new(pool.clone()),
        profiles: Arc::new(ProfilesComponent::new(None, "https://content".to_string())),
        requests: RequestsComponent::new(pool.clone()),
        voice: VoiceComponent::new(pool.clone()),
        pool: pool.clone(),
        mutes_pool: None,
        replay,
        limiter: Arc::new(RateLimiter::new(60, Duration::from_secs(60))),
        gossip: Arc::new(NoopPublisher),
        domain: domains::communities(),
        content_store,
        cdn_url: "https://cdn.example".to_string(),
        global_moderators: vec![],
    });

    (state, content_dir)
}

fn rand_uuid() -> Uuid {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    Uuid::from_bytes(b)
}

async fn seed_community(pool: &PgPool, id: Uuid, owner: &str, private: bool) {
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted) \
         VALUES ($1, $2, $3, $4, $5, TRUE, FALSE)",
    )
    .bind(id)
    .bind("Ban Integrity Community")
    .bind("description")
    .bind(owner)
    .bind(private)
    .execute(pool)
    .await
    .expect("seed community");
}

async fn seed_member(pool: &PgPool, id: Uuid, addr: &str, role: &str) {
    sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role) VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(addr)
    .bind(role)
    .execute(pool)
    .await
    .expect("seed member");
}

async fn seed_ban(pool: &PgPool, id: Uuid, banned: &str, banned_by: &str) {
    sqlx::query(
        "INSERT INTO community_bans (community_id, banned_address, banned_by, reason, active) \
         VALUES ($1, $2, $3, $4, TRUE)",
    )
    .bind(id)
    .bind(banned)
    .bind(banned_by)
    .bind("spam")
    .execute(pool)
    .await
    .expect("seed ban");
}

async fn seed_request(pool: &PgPool, id: Uuid, member: &str, kind: &str) -> Uuid {
    let rid = rand_uuid();
    sqlx::query(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1, $2, $3, 'pending', $4)",
    )
    .bind(rid)
    .bind(id)
    .bind(member)
    .bind(kind)
    .execute(pool)
    .await
    .expect("seed request");
    rid
}

async fn is_member(pool: &PgPool, id: Uuid, addr: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM community_members WHERE community_id = $1 AND member_address = $2)",
    )
    .bind(id)
    .bind(addr)
    .fetch_one(pool)
    .await
    .expect("membership probe")
}

async fn pending_requests(pool: &PgPool, id: Uuid, addr: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM community_requests \
         WHERE community_id = $1 AND member_address = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(addr)
    .fetch_one(pool)
    .await
    .expect("pending probe")
}

fn mk_wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    PrivateKeySigner::from_slice(&key).expect("wallet from bytes")
}

fn wallet_addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

fn link_json(kind: &str, payload: &str, signature: &str) -> String {
    serde_json::json!({
        "type": kind,
        "payload": payload,
        "signature": signature,
    })
    .to_string()
}

async fn signed_headers(wallet: &PrivateKeySigner, method: &str, path: &str) -> HeaderMap {
    let root_addr = wallet_addr(wallet);
    let ephemeral = mk_wallet(250);
    let ephemeral_addr = wallet_addr(&ephemeral);
    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = wallet
        .sign_message(ephemeral_payload.as_bytes())
        .await
        .unwrap();

    let ts_ms = chrono::Utc::now().timestamp_millis();
    let canonical = format!("{}:{}:{}:{}", method, path, ts_ms, "{}").to_lowercase();
    let entity_sig = ephemeral.sign_message(canonical.as_bytes()).await.unwrap();

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
            &ephemeral_sig.to_string(),
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&link_json(
            "ECDSA_SIGNED_ENTITY",
            &canonical,
            &entity_sig.to_string(),
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-timestamp"),
        HeaderValue::from_str(&ts_ms.to_string()).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-metadata"),
        HeaderValue::from_static("{}"),
    );
    headers
}

#[tokio::test]
async fn direct_join_of_private_community_is_rejected() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping direct_join_of_private_community_is_rejected: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let owner = "0x0000000000000000000000000000000000000001";
    let joiner = mk_wallet(77);
    let joiner_addr = wallet_addr(&joiner);

    let private_id = rand_uuid();
    seed_community(&pool, private_id, owner, true).await;
    let headers = signed_headers(
        &joiner,
        "post",
        &format!("/v1/communities/{}/members", private_id),
    )
    .await;
    let resp = writes::add_member(
        State(state.clone()),
        headers,
        Path(private_id.to_string()),
        Bytes::new(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "direct join of a private community must be rejected"
    );
    assert!(
        !is_member(&pool, private_id, &joiner_addr).await,
        "rejected join must not create a membership"
    );

    let public_id = rand_uuid();
    seed_community(&pool, public_id, owner, false).await;
    let headers = signed_headers(
        &joiner,
        "post",
        &format!("/v1/communities/{}/members", public_id),
    )
    .await;
    let resp = writes::add_member(
        State(state.clone()),
        headers,
        Path(public_id.to_string()),
        Bytes::new(),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "public communities still accept direct joins"
    );
    assert!(is_member(&pool, public_id, &joiner_addr).await);

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn banned_user_cannot_create_or_auto_accept_requests() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping banned_user_cannot_create_or_auto_accept_requests: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let owner = "0x0000000000000000000000000000000000000001";
    let banned = mk_wallet(88);
    let banned_addr = wallet_addr(&banned);

    let cid = rand_uuid();
    seed_community(&pool, cid, owner, true).await;
    seed_member(&pool, cid, owner, "owner").await;
    // A pending invite would auto-accept an opposite request_to_join without the ban check.
    seed_request(&pool, cid, &banned_addr, "invite").await;
    seed_ban(&pool, cid, &banned_addr, owner).await;

    let headers = signed_headers(
        &banned,
        "post",
        &format!("/v1/communities/{}/requests", cid),
    )
    .await;
    let (status, _body) = writes::create_request(
        State(state.clone()),
        headers,
        Path(cid.to_string()),
        Bytes::from(serde_json::to_vec(&json!({"type": "request_to_join"})).unwrap()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a banned user must not create requests (nor trigger the auto-accept branch)"
    );
    assert!(
        !is_member(&pool, cid, &banned_addr).await,
        "the auto-accept branch must not have joined the banned user"
    );

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn banning_removes_pending_requests() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping banning_removes_pending_requests: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(99);
    let owner_addr = wallet_addr(&owner);
    let target = "0x00000000000000000000000000000000000000cc";

    let cid = rand_uuid();
    seed_community(&pool, cid, &owner_addr, true).await;
    seed_member(&pool, cid, &owner_addr, "owner").await;
    seed_request(&pool, cid, target, "request_to_join").await;
    assert_eq!(pending_requests(&pool, cid, target).await, 1);

    let headers = signed_headers(
        &owner,
        "post",
        &format!("/v1/communities/{}/members/{}/bans", cid, target),
    )
    .await;
    let resp = client::ban_member(
        State(state.clone()),
        headers,
        Path(client::PathIdAddr {
            id: cid.to_string(),
            address: target.to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "ban must succeed");
    assert_eq!(
        pending_requests(&pool, cid, target).await,
        0,
        "banning must remove the target's pending requests"
    );

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn accepting_a_banned_users_request_is_rejected() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping accepting_a_banned_users_request_is_rejected: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(101);
    let owner_addr = wallet_addr(&owner);
    let target = "0x00000000000000000000000000000000000000dd";

    let cid = rand_uuid();
    seed_community(&pool, cid, &owner_addr, true).await;
    seed_member(&pool, cid, &owner_addr, "owner").await;
    let rid = seed_request(&pool, cid, target, "request_to_join").await;
    seed_ban(&pool, cid, target, &owner_addr).await;

    let headers = signed_headers(
        &owner,
        "patch",
        &format!("/v1/communities/{}/requests/{}", cid, rid),
    )
    .await;
    let resp = client::update_request_status(
        State(state.clone()),
        headers,
        Path(client::PathIdReq {
            id: cid.to_string(),
            request_id: rid.to_string(),
        }),
        Bytes::from(serde_json::to_vec(&json!({"intention": "accepted"})).unwrap()),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "accepting a banned user's request must be rejected"
    );
    assert_eq!(
        pending_requests(&pool, cid, target).await,
        1,
        "the request must be left untouched"
    );
    assert!(
        !is_member(&pool, cid, target).await,
        "no membership may be created for a banned user"
    );

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}
