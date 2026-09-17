use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_contract_gate::{signed_fetch_headers, test_wallet, Wallet};
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};
use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::handlers::communities::{get_community, get_community_v2};
use catalyrst_social_service::rest::handlers::places::get_places;
use catalyrst_social_service::rest::handlers::posts::{get_posts, get_posts_v2};
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

async fn setup_db() -> Option<ScratchSchema> {
    let scratch =
        ScratchSchema::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social_private").await?;
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
    p.push(format!("cmm-private-{}-{}", tag, hex::encode(rnd)));
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

async fn seed_community(pool: &PgPool, id: Uuid, owner: &str, private: bool, unlisted: bool) {
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted) \
         VALUES ($1, $2, 'description', $3, $4, TRUE, $5)",
    )
    .bind(id)
    .bind(format!("community {}", &id.to_string()[..8]))
    .bind(owner)
    .bind(private)
    .bind(unlisted)
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

async fn seed_request(pool: &PgPool, id: Uuid, addr: &str, kind: &str, status: &str) {
    sqlx::query(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(rand_uuid())
    .bind(id)
    .bind(addr)
    .bind(status)
    .bind(kind)
    .execute(pool)
    .await
    .expect("seed request");
}

const OWNER: &str = "0xowner00000000000000000000000000000000000";
const MEMBER: &str = "0xmember0000000000000000000000000000000000";
const STRANGER: &str = "0xstranger00000000000000000000000000000000";
const INVITEE: &str = "0xinvitee000000000000000000000000000000000";
const ASKER: &str = "0xasker00000000000000000000000000000000000";

fn addr(w: &Wallet) -> String {
    w.address().to_lowercase()
}

fn header_map(pairs: Vec<(String, String)>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (k, v) in pairs {
        headers.insert(
            HeaderName::from_bytes(k.as_bytes()).expect("header name"),
            HeaderValue::from_str(&v).expect("header value"),
        );
    }
    headers
}

fn signed_v1(wallet: &Wallet, id: Uuid) -> HeaderMap {
    header_map(signed_fetch_headers(
        wallet,
        "get",
        &format!("/v1/communities/{}", id),
    ))
}

fn signed_v2(wallet: &Wallet, id: Uuid) -> HeaderMap {
    header_map(signed_fetch_headers(
        wallet,
        "get",
        &format!("/v2/communities/{}", id),
    ))
}

async fn v1_status(state: &AppState, headers: HeaderMap, id: Uuid) -> StatusCode {
    get_community(State(state.clone()), headers, Path(id.to_string()))
        .await
        .into_response()
        .status()
}

async fn v2_status(state: &AppState, headers: HeaderMap, id: Uuid) -> StatusCode {
    get_community_v2(State(state.clone()), headers, Path(id.to_string()))
        .await
        .into_response()
        .status()
}

#[tokio::test]
async fn private_community_by_id_is_hidden_from_anonymous_and_non_member_callers() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let comp = CommunitiesComponent::new(pool.clone());

    let private_id = rand_uuid();
    seed_community(&pool, private_id, OWNER, true, false).await;
    seed_member(&pool, private_id, OWNER, "owner").await;
    seed_member(&pool, private_id, MEMBER, "member").await;

    assert!(
        comp.get_by_id(private_id, None)
            .await
            .expect("anonymous read")
            .is_none(),
        "a private community must not be returned to an unauthenticated caller"
    );
    assert!(
        comp.get_by_id(private_id, Some(STRANGER))
            .await
            .expect("stranger read")
            .is_none(),
        "a private community must not be returned to a signed non-member"
    );
    assert!(
        comp.get_by_id(private_id, Some(MEMBER))
            .await
            .expect("member read")
            .is_some(),
        "a member keeps reading the private community without holding any invite"
    );
    assert!(
        comp.get_by_id(private_id, Some(OWNER))
            .await
            .expect("owner read")
            .is_some(),
        "the owner keeps reading the private community"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn public_and_unlisted_communities_stay_reachable_by_id() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let comp = CommunitiesComponent::new(pool.clone());

    let public_id = rand_uuid();
    let unlisted_id = rand_uuid();
    seed_community(&pool, public_id, OWNER, false, false).await;
    seed_community(&pool, unlisted_id, OWNER, false, true).await;

    assert!(
        comp.get_by_id(public_id, None)
            .await
            .expect("public read")
            .is_some(),
        "public communities stay readable anonymously"
    );
    assert!(
        comp.get_by_id(unlisted_id, None)
            .await
            .expect("unlisted read")
            .is_some(),
        "unlisted means hidden from discovery, not from someone holding the id"
    );
    assert!(
        comp.get_by_id(unlisted_id, Some(STRANGER))
            .await
            .expect("unlisted signed read")
            .is_some(),
        "a signed non-member still reads an unlisted public community by id"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn only_a_pending_invite_admits_a_non_member_to_a_private_community() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let comp = CommunitiesComponent::new(pool.clone());

    let private_id = rand_uuid();
    seed_community(&pool, private_id, OWNER, true, false).await;
    seed_member(&pool, private_id, OWNER, "owner").await;
    seed_member(&pool, private_id, MEMBER, "member").await;
    seed_voice_chat(&pool, private_id, 3, 1).await;
    seed_request(&pool, private_id, INVITEE, "invite", "pending").await;
    seed_request(&pool, private_id, ASKER, "request_to_join", "pending").await;
    seed_request(&pool, private_id, STRANGER, "invite", "rejected").await;

    let invited = comp
        .get_by_id(private_id, Some(INVITEE))
        .await
        .expect("invitee read")
        .expect("a pending invite names the community, so the invitee may read it");
    assert_eq!(
        invited.role.as_deref(),
        Some("none"),
        "the invitee is admitted without becoming a member"
    );
    assert_eq!(invited.privacy, "private");
    assert!(
        invited.is_live,
        "an admitted caller reads the running voice chat, whatever role admitted them"
    );
    assert_eq!(invited.voice_chat_status.participant_count, 3);
    assert_eq!(invited.voice_chat_status.moderator_count, 1);

    let as_member = comp
        .get_by_id(private_id, Some(MEMBER))
        .await
        .expect("member read")
        .expect("a member reads the private community");
    assert!(as_member.is_live);
    assert_eq!(
        as_member.voice_chat_status.participant_count, invited.voice_chat_status.participant_count,
        "the invitee and the member read the same voice chat state"
    );
    assert_eq!(
        as_member.voice_chat_status.moderator_count,
        invited.voice_chat_status.moderator_count
    );

    assert!(
        comp.get_by_id(private_id, Some(ASKER))
            .await
            .expect("asker read")
            .is_none(),
        "a pending request to join is not an invite and must not admit the caller"
    );
    assert!(
        comp.get_by_id(private_id, Some(STRANGER))
            .await
            .expect("rejected invitee read")
            .is_none(),
        "a rejected invite must not admit the caller"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn both_by_id_versions_answer_404_for_private_communities() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let stranger = test_wallet(21);
    let member = test_wallet(23);
    let private_id = rand_uuid();
    seed_community(&pool, private_id, OWNER, true, false).await;
    seed_member(&pool, private_id, OWNER, "owner").await;
    seed_member(&pool, private_id, &addr(&member), "member").await;

    assert_eq!(
        v1_status(&state, HeaderMap::new(), private_id).await,
        StatusCode::NOT_FOUND,
        "GET /v1/communities/{{id}} must not answer 200 to an anonymous caller"
    );
    assert_eq!(
        v2_status(&state, HeaderMap::new(), private_id).await,
        StatusCode::NOT_FOUND,
        "GET /v2/communities/{{id}} must not answer 200 to an anonymous caller"
    );
    assert_eq!(
        v1_status(&state, signed_v1(&stranger, private_id), private_id).await,
        StatusCode::NOT_FOUND,
        "GET /v1/communities/{{id}} must not answer 200 to a signed non-member"
    );
    assert_eq!(
        v2_status(&state, signed_v2(&stranger, private_id), private_id).await,
        StatusCode::NOT_FOUND,
        "GET /v2/communities/{{id}} must not answer 200 to a signed non-member"
    );
    assert_eq!(
        v1_status(&state, signed_v1(&member, private_id), private_id).await,
        StatusCode::OK,
        "a member still reads the private community on v1"
    );
    assert_eq!(
        v2_status(&state, signed_v2(&member, private_id), private_id).await,
        StatusCode::OK,
        "a member still reads the private community on v2"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn both_by_id_versions_admit_a_pending_invitee() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let invitee = test_wallet(25);
    let private_id = rand_uuid();
    seed_community(&pool, private_id, OWNER, true, false).await;
    seed_member(&pool, private_id, OWNER, "owner").await;
    seed_request(&pool, private_id, &addr(&invitee), "invite", "pending").await;

    assert_eq!(
        v1_status(&state, signed_v1(&invitee, private_id), private_id).await,
        StatusCode::OK,
        "the pending invitee reads the private community on v1"
    );
    assert_eq!(
        v2_status(&state, signed_v2(&invitee, private_id), private_id).await,
        StatusCode::OK,
        "the pending invitee reads the private community on v2"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

async fn seed_post(pool: &PgPool, id: Uuid, author: &str) {
    sqlx::query(
        "INSERT INTO community_posts (id, community_id, author_address, content) \
         VALUES ($1, $2, $3, 'a post only members should read')",
    )
    .bind(rand_uuid())
    .bind(id)
    .bind(author)
    .execute(pool)
    .await
    .expect("seed post");
}

async fn seed_place(pool: &PgPool, id: Uuid, added_by: &str) {
    sqlx::query("INSERT INTO community_places (id, community_id, added_by) VALUES ($1, $2, $3)")
        .bind(rand_uuid().to_string())
        .bind(id)
        .bind(added_by)
        .execute(pool)
        .await
        .expect("seed place");
}

async fn seed_voice_chat(pool: &PgPool, id: Uuid, participants: i32, moderators: i32) {
    sqlx::query(
        "INSERT INTO community_voice_chats (community_id, participants, moderators) \
         VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(participants)
    .bind(moderators)
    .execute(pool)
    .await
    .expect("seed voice chat");
}

fn signed_for(wallet: &Wallet, path: &str) -> HeaderMap {
    header_map(signed_fetch_headers(wallet, "get", path))
}

async fn posts_v1_status(state: &AppState, headers: HeaderMap, id: Uuid) -> StatusCode {
    get_posts(
        State(state.clone()),
        headers,
        Path(id.to_string()),
        Query(Vec::new()),
    )
    .await
    .into_response()
    .status()
}

async fn posts_v2_status(state: &AppState, headers: HeaderMap, id: Uuid) -> StatusCode {
    get_posts_v2(
        State(state.clone()),
        headers,
        Path(id.to_string()),
        Query(Vec::new()),
    )
    .await
    .into_response()
    .status()
}

async fn places_v1_status(state: &AppState, headers: HeaderMap, id: Uuid) -> StatusCode {
    get_places(
        State(state.clone()),
        headers,
        Path(id.to_string()),
        Query(Vec::new()),
    )
    .await
    .into_response()
    .status()
}

#[tokio::test]
async fn private_community_posts_and_places_refuse_a_signed_non_member() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let stranger = test_wallet(31);
    let member = test_wallet(33);
    let private_id = rand_uuid();
    seed_community(&pool, private_id, OWNER, true, false).await;
    seed_member(&pool, private_id, OWNER, "owner").await;
    seed_member(&pool, private_id, &addr(&member), "member").await;
    seed_post(&pool, private_id, OWNER).await;
    seed_place(&pool, private_id, OWNER).await;

    let posts_v1 = format!("/v1/communities/{}/posts", private_id);
    let posts_v2 = format!("/v2/communities/{}/posts", private_id);
    let places_v1 = format!("/v1/communities/{}/places", private_id);

    assert_eq!(
        posts_v1_status(&state, HeaderMap::new(), private_id).await,
        StatusCode::NOT_FOUND,
        "an anonymous caller must not read a private community's posts"
    );
    assert_eq!(
        posts_v2_status(&state, HeaderMap::new(), private_id).await,
        StatusCode::NOT_FOUND,
        "an anonymous caller must not read a private community's posts on v2"
    );
    assert_eq!(
        places_v1_status(&state, HeaderMap::new(), private_id).await,
        StatusCode::NOT_FOUND,
        "an anonymous caller must not read a private community's places"
    );

    assert_eq!(
        posts_v1_status(&state, signed_for(&stranger, &posts_v1), private_id).await,
        StatusCode::UNAUTHORIZED,
        "a signed non-member must not read a private community's posts"
    );
    assert_eq!(
        posts_v2_status(&state, signed_for(&stranger, &posts_v2), private_id).await,
        StatusCode::UNAUTHORIZED,
        "a signed non-member must not read a private community's posts on v2"
    );
    assert_eq!(
        places_v1_status(&state, signed_for(&stranger, &places_v1), private_id).await,
        StatusCode::UNAUTHORIZED,
        "a signed non-member must not read a private community's places"
    );

    assert_eq!(
        posts_v1_status(&state, signed_for(&member, &posts_v1), private_id).await,
        StatusCode::OK,
        "a member still reads the private community's posts"
    );
    assert_eq!(
        posts_v2_status(&state, signed_for(&member, &posts_v2), private_id).await,
        StatusCode::OK,
        "a member still reads the private community's posts on v2"
    );
    assert_eq!(
        places_v1_status(&state, signed_for(&member, &places_v1), private_id).await,
        StatusCode::OK,
        "a member still reads the private community's places"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn public_community_posts_and_places_stay_open_to_everyone() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let stranger = test_wallet(35);
    let public_id = rand_uuid();
    seed_community(&pool, public_id, OWNER, false, false).await;
    seed_member(&pool, public_id, OWNER, "owner").await;
    seed_post(&pool, public_id, OWNER).await;
    seed_place(&pool, public_id, OWNER).await;

    let posts_v1 = format!("/v1/communities/{}/posts", public_id);
    let posts_v2 = format!("/v2/communities/{}/posts", public_id);
    let places_v1 = format!("/v1/communities/{}/places", public_id);

    for (label, status) in [
        (
            "anonymous posts v1",
            posts_v1_status(&state, HeaderMap::new(), public_id).await,
        ),
        (
            "anonymous posts v2",
            posts_v2_status(&state, HeaderMap::new(), public_id).await,
        ),
        (
            "anonymous places v1",
            places_v1_status(&state, HeaderMap::new(), public_id).await,
        ),
        (
            "signed non-member posts v1",
            posts_v1_status(&state, signed_for(&stranger, &posts_v1), public_id).await,
        ),
        (
            "signed non-member posts v2",
            posts_v2_status(&state, signed_for(&stranger, &posts_v2), public_id).await,
        ),
        (
            "signed non-member places v1",
            places_v1_status(&state, signed_for(&stranger, &places_v1), public_id).await,
        ),
    ] {
        assert_eq!(
            status,
            StatusCode::OK,
            "the private gate must not over-refuse a public community ({label})"
        );
    }

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}
