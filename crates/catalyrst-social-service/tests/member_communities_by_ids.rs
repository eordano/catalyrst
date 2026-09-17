use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::Json;
use catalyrst_contract_gate::pg::ScratchSchema;
use rand::Rng;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};
use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::handlers::writes::member_communities_by_ids;
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

const ADMIN_TOKEN: &str = "cg-social-batch-admin";
const OWNER: &str = "0xowner00000000000000000000000000000000000";
const MEMBER: &str = "0xmember0000000000000000000000000000000000";
const OTHER: &str = "0xother00000000000000000000000000000000000";

async fn setup_db() -> Option<ScratchSchema> {
    let scratch =
        ScratchSchema::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social_batch").await?;
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

fn unique_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rnd = [0u8; 8];
    rand::rng().fill_bytes(&mut rnd);
    p.push(format!("cmm-batch-{}-{}", tag, hex::encode(rnd)));
    p
}

fn rand_uuid() -> Uuid {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    Uuid::from_bytes(b)
}

async fn build_state(pool: &PgPool) -> (AppState, PathBuf) {
    let content_dir = unique_dir("state");
    let content_store = Arc::new(ContentStore::new(&content_dir, MAX_BODY_BYTES));
    content_store.init().await.expect("content store init");
    let replay = Replay::new(pool.clone()).await.expect("replay init");

    let state = Arc::new(AppStateInner {
        admin_token: Some(ADMIN_TOKEN.to_string()),
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

async fn seed_community(pool: &PgPool, id: Uuid, private: bool, unlisted: bool, active: bool) {
    seed_community_with_suspension(pool, id, private, unlisted, active, false).await;
}

async fn seed_community_with_suspension(
    pool: &PgPool,
    id: Uuid,
    private: bool,
    unlisted: bool,
    active: bool,
    suspended: bool,
) {
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted, suspended) \
         VALUES ($1, $2, 'description', $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(format!("community {}", &id.to_string()[..8]))
    .bind(OWNER)
    .bind(private)
    .bind(active)
    .bind(unlisted)
    .bind(suspended)
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

async fn seed_ban(pool: &PgPool, id: Uuid, banned: &str) {
    sqlx::query(
        "INSERT INTO community_bans (community_id, banned_address, banned_by, reason, active) \
         VALUES ($1, $2, $3, 'spam', TRUE)",
    )
    .bind(id)
    .bind(banned)
    .bind(OWNER)
    .execute(pool)
    .await
    .expect("seed ban");
}

struct Seeded {
    public_joined: Uuid,
    public_not_joined: Uuid,
    private_joined: Uuid,
    private_not_joined: Uuid,
    unlisted_joined: Uuid,
    banned_membership: Uuid,
    inactive_joined: Uuid,
    moderator_joined: Uuid,
    suspended_joined: Uuid,
    none_role_joined: Uuid,
}

async fn seed_world(pool: &PgPool) -> Seeded {
    let seeded = Seeded {
        public_joined: rand_uuid(),
        public_not_joined: rand_uuid(),
        private_joined: rand_uuid(),
        private_not_joined: rand_uuid(),
        unlisted_joined: rand_uuid(),
        banned_membership: rand_uuid(),
        inactive_joined: rand_uuid(),
        moderator_joined: rand_uuid(),
        suspended_joined: rand_uuid(),
        none_role_joined: rand_uuid(),
    };

    seed_community(pool, seeded.public_joined, false, false, true).await;
    seed_community(pool, seeded.public_not_joined, false, false, true).await;
    seed_community(pool, seeded.private_joined, true, false, true).await;
    seed_community(pool, seeded.private_not_joined, true, false, true).await;
    seed_community(pool, seeded.unlisted_joined, false, true, true).await;
    seed_community(pool, seeded.banned_membership, false, false, true).await;
    seed_community(pool, seeded.inactive_joined, false, false, false).await;
    seed_community(pool, seeded.moderator_joined, false, false, true).await;
    seed_community_with_suspension(pool, seeded.suspended_joined, false, false, true, true).await;
    seed_community(pool, seeded.none_role_joined, false, false, true).await;

    seed_member(pool, seeded.public_joined, MEMBER, "member").await;
    seed_member(pool, seeded.private_joined, MEMBER, "owner").await;
    seed_member(pool, seeded.unlisted_joined, MEMBER, "member").await;
    seed_member(pool, seeded.banned_membership, MEMBER, "member").await;
    seed_member(pool, seeded.inactive_joined, MEMBER, "member").await;
    seed_member(pool, seeded.moderator_joined, MEMBER, "mod").await;
    seed_member(pool, seeded.suspended_joined, MEMBER, "member").await;
    seed_member(pool, seeded.public_not_joined, OTHER, "member").await;
    seed_member(pool, seeded.none_role_joined, MEMBER, "none").await;
    seed_ban(pool, seeded.banned_membership, MEMBER).await;

    seeded
}

fn all_ids(seeded: &Seeded) -> Vec<Uuid> {
    vec![
        seeded.public_joined,
        seeded.public_not_joined,
        seeded.private_joined,
        seeded.private_not_joined,
        seeded.unlisted_joined,
        seeded.banned_membership,
        seeded.inactive_joined,
        seeded.moderator_joined,
        seeded.suspended_joined,
        seeded.none_role_joined,
    ]
}

#[tokio::test]
async fn only_actual_memberships_come_back_with_their_role() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let rows = comp
        .member_communities_by_ids(&all_ids(&seeded), MEMBER)
        .await
        .expect("batch read");
    let mut found: Vec<(Uuid, String)> = rows;
    found.sort_by_key(|(id, _)| *id);

    let role_of = |id: Uuid| -> Option<String> {
        found
            .iter()
            .find(|(found_id, _)| *found_id == id)
            .map(|(_, role)| role.clone())
    };

    assert_eq!(
        role_of(seeded.public_joined).as_deref(),
        Some("member"),
        "a joined public community is returned with its role"
    );
    assert_eq!(
        role_of(seeded.private_joined).as_deref(),
        Some("owner"),
        "privacy does not matter once the address is a member"
    );
    assert_eq!(
        role_of(seeded.unlisted_joined).as_deref(),
        Some("member"),
        "listing does not matter once the address is a member"
    );
    assert_eq!(
        role_of(seeded.moderator_joined).as_deref(),
        Some("moderator"),
        "the stored role text is normalized to the owner|moderator|member contract"
    );
    assert_eq!(
        role_of(seeded.public_not_joined),
        None,
        "a listed community the address never joined must not come back as a membership"
    );
    assert_eq!(
        role_of(seeded.private_not_joined),
        None,
        "a community the address never joined is never returned"
    );
    assert_eq!(
        role_of(seeded.banned_membership),
        None,
        "a banned membership is excluded"
    );
    assert_eq!(
        role_of(seeded.inactive_joined),
        None,
        "an inactive community is excluded"
    );
    assert_eq!(
        role_of(seeded.suspended_joined),
        None,
        "a suspended community is withdrawn from the batch as it is from every by-id read"
    );
    assert_eq!(
        role_of(seeded.none_role_joined),
        None,
        "a row stored at the role `none` is not a membership and must never be reported as one"
    );
    assert_eq!(found.len(), 4, "exactly the four memberships: {found:?}");

    scratch.drop().await;
}

#[tokio::test]
async fn a_different_address_gets_none_of_the_memberships() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let seeded = seed_world(&pool).await;
    let comp = CommunitiesComponent::new(pool.clone());

    let rows = comp
        .member_communities_by_ids(
            &all_ids(&seeded),
            "0xnobody00000000000000000000000000000000",
        )
        .await
        .expect("batch read");
    assert!(
        rows.is_empty(),
        "an unrelated address holds nothing: {rows:?}"
    );

    let empty = comp
        .member_communities_by_ids(&[], MEMBER)
        .await
        .expect("empty batch");
    assert!(empty.is_empty(), "an empty batch stays empty");

    scratch.drop().await;
}

#[tokio::test]
async fn the_batch_endpoint_answers_with_id_and_role() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let seeded = seed_world(&pool).await;
    let (state, dir) = build_state(&pool).await;

    let ids: Vec<String> = all_ids(&seeded).iter().map(|id| id.to_string()).collect();

    let (status, Json(payload)) = post_batch(&state, MEMBER, json!({ "communityIds": ids })).await;
    assert_eq!(status, StatusCode::OK);

    let communities = payload["data"]["communities"]
        .as_array()
        .expect("communities array");
    assert_eq!(communities.len(), 4, "only memberships: {communities:?}");
    for entry in communities {
        let obj = entry.as_object().expect("entry object");
        assert!(obj.contains_key("id"), "entry keeps the id: {entry}");
        let role = obj
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or_else(|| panic!("entry must carry the role: {entry}"));
        assert!(
            matches!(role, "owner" | "moderator" | "member"),
            "role must be one of owner|moderator|member, got {role}"
        );
    }
    let returned: Vec<&str> = communities
        .iter()
        .filter_map(|c| c["id"].as_str())
        .collect();
    assert!(
        !returned.contains(&seeded.public_not_joined.to_string().as_str()),
        "a merely visible community must not appear: {returned:?}"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

fn admin_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", ADMIN_TOKEN)).unwrap(),
    );
    headers
}

async fn post_batch(
    state: &AppState,
    address: &str,
    body: serde_json::Value,
) -> (StatusCode, Json<serde_json::Value>) {
    post_batch_raw(state, address, Bytes::from(body.to_string())).await
}

async fn post_batch_raw(
    state: &AppState,
    address: &str,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    member_communities_by_ids(
        State(state.clone()),
        admin_headers(),
        Path(address.to_string()),
        body,
    )
    .await
}

#[tokio::test]
async fn the_batch_endpoint_refuses_bodies_the_upstream_schema_rejects() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let seeded = seed_world(&pool).await;
    let (state, dir) = build_state(&pool).await;

    let (status, _) = post_batch_raw(&state, MEMBER, Bytes::new()).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a missing body is a malformed request, not an empty membership set"
    );

    let (status, _) = post_batch(&state, MEMBER, json!({})).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "communityIds is required by the upstream schema"
    );

    let (status, _) = post_batch(&state, MEMBER, json!({ "communityIds": [] })).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "minItems 1: an empty batch is refused"
    );

    let (status, _) = post_batch(
        &state,
        MEMBER,
        json!({ "communityIds": [seeded.public_joined.to_string()], "extra": 1 }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "additionalProperties false: an unknown field is refused"
    );

    let (status, Json(payload)) = post_batch(
        &state,
        MEMBER,
        json!({ "communityIds": [seeded.public_joined.to_string(), "not-a-uuid"] }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a malformed id is refused rather than silently dropped"
    );
    assert!(
        payload.get("data").is_none(),
        "the valid ids must not be answered alongside a rejected one: {payload}"
    );

    let unhyphenated = seeded.public_joined.simple().to_string();
    let (status, Json(payload)) = post_batch(
        &state,
        MEMBER,
        json!({ "communityIds": [unhyphenated.clone()] }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "UUID_PATTERN pins the hyphenated spelling: {unhyphenated} is refused"
    );
    assert!(
        payload.get("data").is_none(),
        "a refused spelling answers no membership set: {payload}"
    );

    let over: Vec<String> = (0..51).map(|_| rand_uuid().to_string()).collect();
    let (status, _) = post_batch(&state, MEMBER, json!({ "communityIds": over })).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "maxItems 50: an oversized batch is refused"
    );

    let mut at_limit: Vec<String> = (0..49).map(|_| rand_uuid().to_string()).collect();
    at_limit.push(seeded.public_joined.to_string());
    let (status, Json(payload)) =
        post_batch(&state, MEMBER, json!({ "communityIds": at_limit })).await;
    assert_eq!(status, StatusCode::OK, "exactly 50 ids is accepted");
    assert_eq!(
        payload["data"]["communities"]
            .as_array()
            .expect("communities array")
            .len(),
        1,
        "the one real membership in the batch comes back"
    );

    let (status, _) = member_communities_by_ids(
        State(state.clone()),
        HeaderMap::new(),
        Path(MEMBER.to_string()),
        Bytes::new(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the admin bearer gate still runs before any body validation"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}
