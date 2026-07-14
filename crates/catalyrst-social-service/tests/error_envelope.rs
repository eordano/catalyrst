use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use catalyrst_contract_gate::pg::ScratchSchema;
use rand::Rng;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};
use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::handlers::communities::get_community;
use catalyrst_social_service::rest::handlers::mutes::get_mutes;
use catalyrst_social_service::rest::ports::bans::BansComponent;
use catalyrst_social_service::rest::ports::communities::CommunitiesComponent;
use catalyrst_social_service::rest::ports::invites::InvitesComponent;
use catalyrst_social_service::rest::ports::members::MembersComponent;
use catalyrst_social_service::rest::ports::moderation::ModerationComponent;
use catalyrst_social_service::rest::ports::peers_stats::PeersStatsClient;
use catalyrst_social_service::rest::ports::places::PlacesComponent;
use catalyrst_social_service::rest::ports::places_api::PlacesApiClient;
use catalyrst_social_service::rest::ports::posts::PostsComponent;
use catalyrst_social_service::rest::ports::profiles::ProfilesComponent;
use catalyrst_social_service::rest::ports::requests::RequestsComponent;
use catalyrst_social_service::rest::ports::voice::VoiceComponent;
use catalyrst_social_service::rest::{AppState, AppStateInner};

fn unique_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rnd = [0u8; 8];
    rand::rng().fill_bytes(&mut rnd);
    p.push(format!("cmm-envelope-{}-{}", tag, hex::encode(rnd)));
    p
}

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create_or_default(
        "CATALYRST_SOCIAL_SERVICE_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5434/communities",
        "cg_social_envelope",
    )
    .await?;
    for sql in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0002_federation.sql"),
        include_str!("../migrations/0003_voice_moderators.sql"),
        include_str!("../migrations/0004_thumbnail_hash.sql"),
        include_str!("../migrations/0005_suspension.sql"),
        include_str!("../migrations/0006_role_check_reconcile.sql"),
    ] {
        apply_migration(&scratch.pool, sql).await;
    }
    Some(scratch)
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
        if line.trim_start().starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn build_state(pool: &PgPool) -> (AppState, PathBuf) {
    let content_dir = unique_dir("state");
    let content_store = Arc::new(ContentStore::new(&content_dir, MAX_BODY_BYTES));
    content_store.init().await.expect("content store init");
    let replay = Replay::new(pool.clone()).await.expect("replay init");

    let state = Arc::new(AppStateInner {
        admin_token: None,
        bans: BansComponent::new(pool.clone()),
        communities: CommunitiesComponent::new(pool.clone()),
        invites: InvitesComponent::new(pool.clone()),
        members: MembersComponent::new(pool.clone()),
        moderation: ModerationComponent::new(pool.clone()),
        peers_stats: PeersStatsClient::new("http://127.0.0.1:1".to_string()),
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
        restricted_names: vec![],
        gatekeeper: Gatekeeper::with_token("http://127.0.0.1:1".to_string(), None),
    });

    (state, content_dir)
}

async fn json_of(resp: axum::response::Response) -> (StatusCode, Value) {
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .expect("body");
    let v: Value = serde_json::from_slice(&bytes).expect("json body");
    (status, v)
}

fn assert_unified_envelope(v: &Value) {
    assert_eq!(v["ok"], Value::Bool(false), "ok must be false: {v}");
    assert!(
        v.get("error").and_then(Value::as_str).is_some(),
        "unified envelope must carry a string `error` field: {v}"
    );
    assert!(
        v.get("message").and_then(Value::as_str).is_some(),
        "unified envelope must carry a string `message` field: {v}"
    );
}

#[tokio::test]
async fn community_404_carries_unified_envelope() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let (state, dir) = build_state(&scratch.pool).await;

    let mut missing_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut missing_bytes);
    let missing = Uuid::from_bytes(missing_bytes);
    let resp = get_community(
        State(state.clone()),
        HeaderMap::new(),
        Path(missing.to_string()),
    )
    .await
    .into_response();

    let (status, body) = json_of(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_unified_envelope(&body);
    assert_eq!(body["error"], "Not Found");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("Community not found"),
        "message keeps the not-found detail: {body}"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn mutes_error_carries_unified_envelope() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let (state, dir) = build_state(&scratch.pool).await;

    let resp = get_mutes(State(state.clone()), HeaderMap::new(), Query(Vec::new()))
        .await
        .into_response();

    let (status, body) = json_of(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_unified_envelope(&body);

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}
