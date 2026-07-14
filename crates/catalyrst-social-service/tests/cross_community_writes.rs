use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_contract_gate::pg::ScratchSchema;
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{GossipEnvelope, NoopPublisher, RateLimiter, Scope, Signed, TypedMessage};
use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::community_membership_authority::{
    load_standing_from_community_role_current, CommunityMembershipTier,
};
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::ids::signature_hash_hex;
use catalyrst_social_service::rest::fed::messages::{
    CommunityCreate, CommunityPost, CommunityPostDelete, CommunityPostLike, CommunityPostUnlike,
    CommunityRequestStatusUpdate,
};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::fed::{apply, consumer};
use catalyrst_social_service::rest::handlers::writes;
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
    p.push(format!("cmm-xcw-{}-{}", tag, hex::encode(rnd)));
    p
}

async fn setup_db(tag: &str) -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create_or_default(
        "CATALYRST_SOCIAL_SERVICE_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/communities",
        tag,
    )
    .await?;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0001_initial.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0002_federation.sql"),
    )
    .await;
    Some(scratch)
}

async fn apply_migration(pool: &PgPool, sql: &str) {
    let cleaned = strip_line_comments(sql);
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

fn strip_line_comments(s: &str) -> String {
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
        limiter: Arc::new(RateLimiter::new(600, Duration::from_secs(60))),
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

fn mk_wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    PrivateKeySigner::from_slice(&key).expect("wallet from bytes")
}

fn wallet_addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

fn rand_nonce() -> [u8; 16] {
    let mut n = [0u8; 16];
    rand::rng().fill_bytes(&mut n);
    n
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn sign<T: TypedMessage>(
    wallet: &PrivateKeySigner,
    message: T,
    domain: Eip712Domain,
) -> Signed<T> {
    let mut signed = Signed {
        domain,
        message,
        nonce: rand_nonce(),
        signed_at: now(),
        signature: String::new(),
    };
    let hash = signed.hash();
    signed.signature = wallet.sign_message(&hash).await.unwrap().to_string();
    signed
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

struct Community {
    hex: String,
    uuid: Uuid,
}

async fn found_community(
    pool: &PgPool,
    owner: &PrivateKeySigner,
    name: &str,
    private: bool,
) -> Community {
    let domain = domains::communities();
    let create = sign(
        owner,
        CommunityCreate {
            name: name.to_string(),
            description: "cross-community write fixture".into(),
            private,
            unlisted: false,
            flags: vec![],
        },
        domain,
    )
    .await;
    let applied = apply::apply_create(pool, &create, &wallet_addr(owner))
        .await
        .expect("found community");
    Community {
        hex: applied.community_id,
        uuid: applied.uuid,
    }
}

async fn publish_post(pool: &PgPool, author: &PrivateKeySigner, community_hex: &str) -> String {
    let domain = domains::communities();
    let post = sign(
        author,
        CommunityPost {
            community_id: community_hex.to_string(),
            content_hash: "bafkreipostfixturecontenthash".into(),
        },
        domain,
    )
    .await;
    apply::apply_post(pool, &post, &wallet_addr(author))
        .await
        .expect("publish post")
}

async fn seed_request(pool: &PgPool, community: Uuid, member: &str, kind: &str) -> Uuid {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    let rid = Uuid::from_bytes(b);
    sqlx::query(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1, $2, $3, 'pending', $4)",
    )
    .bind(rid)
    .bind(community)
    .bind(member)
    .bind(kind)
    .execute(pool)
    .await
    .expect("seed request");
    rid
}

async fn request_status(pool: &PgPool, request: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM community_requests WHERE id = $1")
        .bind(request)
        .fetch_one(pool)
        .await
        .expect("request status probe")
}

async fn likes_on(pool: &PgPool, post_sig: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM community_post_likes_log WHERE post_signature_hash = $1",
    )
    .bind(post_sig)
    .fetch_one(pool)
    .await
    .expect("likes probe")
}

async fn active_likes_on(pool: &PgPool, post_sig: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM community_post_likes_log \
          WHERE post_signature_hash = $1 AND unliked_by_sig IS NULL",
    )
    .bind(post_sig)
    .fetch_one(pool)
    .await
    .expect("active likes probe")
}

async fn post_is_deleted(pool: &PgPool, post_sig: &str) -> bool {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT deleted_by_sig FROM community_posts_log WHERE signature_hash = $1",
    )
    .bind(post_sig)
    .fetch_one(pool)
    .await
    .expect("post deletion probe")
    .is_some()
}

fn envelope<T: TypedMessage + serde::Serialize>(
    signed: &Signed<T>,
    signer: &str,
) -> GossipEnvelope {
    GossipEnvelope::local(
        Scope::Communities,
        signed,
        signature_hash_hex(&signed.hash()),
        signer.to_ascii_lowercase(),
    )
    .expect("build gossip envelope")
}

#[tokio::test]
async fn rest_request_status_cannot_reach_another_communitys_request() {
    let Some(scratch) = setup_db("cg_social_xcw_req").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let victim_owner = mk_wallet(140);
    let attacker = mk_wallet(141);
    let attacker_addr = wallet_addr(&attacker);
    let applicant = "0x00000000000000000000000000000000000000aa";

    let victim = found_community(&pool, &victim_owner, "VictimCommunity", true).await;
    let attacker_home = found_community(&pool, &attacker, "AttackerCommunity", true).await;
    let request_id = seed_request(&pool, victim.uuid, applicant, "request_to_join").await;

    assert_eq!(
        load_standing_from_community_role_current(&pool, &attacker_home.hex, &attacker_addr)
            .await
            .unwrap()
            .tier(),
        CommunityMembershipTier::OwnerOfThisCommunity,
        "the attacker really is an owner of the community they name"
    );
    assert_eq!(
        load_standing_from_community_role_current(&pool, &victim.hex, &attacker_addr)
            .await
            .unwrap()
            .tier(),
        CommunityMembershipTier::NotAMemberOfThisCommunity,
        "the attacker holds no role in the community that owns the request"
    );

    let signed = sign(
        &attacker,
        CommunityRequestStatusUpdate {
            community_id: attacker_home.hex.clone(),
            request_id: request_id.to_string(),
            status: "accepted".into(),
        },
        domains::communities(),
    )
    .await;
    let path = format!(
        "/v1/communities/{}/requests/{}",
        attacker_home.uuid, request_id
    );
    let headers = signed_headers(&attacker, "patch", &path).await;
    let resp = writes::update_request_status(
        State(state.clone()),
        headers,
        Path(writes::PathIdReq {
            id: attacker_home.uuid.to_string(),
            request_id: request_id.to_string(),
        }),
        Bytes::from(serde_json::to_vec(&signed).unwrap()),
    )
    .await;

    assert_eq!(
        request_status(&pool, request_id).await,
        "pending",
        "a moderator of another community must not rewrite this request"
    );
    assert!(
        !resp.status().is_success(),
        "the cross-community request update must be refused, got {}",
        resp.status()
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn rest_request_status_vocabulary_is_closed() {
    let Some(scratch) = setup_db("cg_social_xcw_vocab").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(142);
    let applicant = "0x00000000000000000000000000000000000000ab";
    let home = found_community(&pool, &owner, "VocabularyCommunity", true).await;
    let request_id = seed_request(&pool, home.uuid, applicant, "request_to_join").await;

    let signed = sign(
        &owner,
        CommunityRequestStatusUpdate {
            community_id: home.hex.clone(),
            request_id: request_id.to_string(),
            status: "owner".into(),
        },
        domains::communities(),
    )
    .await;
    let path = format!("/v1/communities/{}/requests/{}", home.uuid, request_id);
    let headers = signed_headers(&owner, "patch", &path).await;
    let resp = writes::update_request_status(
        State(state.clone()),
        headers,
        Path(writes::PathIdReq {
            id: home.uuid.to_string(),
            request_id: request_id.to_string(),
        }),
        Bytes::from(serde_json::to_vec(&signed).unwrap()),
    )
    .await;

    assert_eq!(
        request_status(&pool, request_id).await,
        "pending",
        "an off-vocabulary status must not be written into community_requests"
    );
    assert!(
        !resp.status().is_success(),
        "an off-vocabulary status must be refused, got {}",
        resp.status()
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn rest_like_cannot_reach_another_communitys_post() {
    let Some(scratch) = setup_db("cg_social_xcw_like").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let victim_owner = mk_wallet(143);
    let attacker = mk_wallet(144);
    let attacker_addr = wallet_addr(&attacker);

    let victim = found_community(&pool, &victim_owner, "PrivatePostsCommunity", true).await;
    let attacker_home = found_community(&pool, &attacker, "AttackerLikeCommunity", false).await;
    let post_sig = publish_post(&pool, &victim_owner, &victim.hex).await;

    assert_eq!(
        load_standing_from_community_role_current(&pool, &victim.hex, &attacker_addr)
            .await
            .unwrap()
            .tier(),
        CommunityMembershipTier::NotAMemberOfThisCommunity,
        "the attacker is an outsider to the private community holding the post"
    );

    let signed = sign(
        &attacker,
        CommunityPostLike {
            community_id: attacker_home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    let path = format!(
        "/v1/communities/{}/posts/{}/like",
        attacker_home.uuid, post_sig
    );
    let headers = signed_headers(&attacker, "post", &path).await;
    let resp = writes::like_post(
        State(state.clone()),
        headers,
        Path(writes::PathIdPost {
            id: attacker_home.uuid.to_string(),
            post_id: post_sig.clone(),
        }),
        Bytes::from(serde_json::to_vec(&signed).unwrap()),
    )
    .await;

    assert_eq!(
        likes_on(&pool, &post_sig).await,
        0,
        "an outsider must not register a like on a private community's post"
    );
    assert!(
        !resp.status().is_success(),
        "the cross-community like must be refused, got {}",
        resp.status()
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn gossip_post_delete_cannot_reach_another_communitys_post() {
    let Some(scratch) = setup_db("cg_social_xcw_gdel").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let victim_owner = mk_wallet(145);
    let attacker = mk_wallet(146);

    let victim = found_community(&pool, &victim_owner, "VictimPostsCommunity", true).await;
    let attacker_home = found_community(&pool, &attacker, "AttackerDeleteCommunity", true).await;
    let post_sig = publish_post(&pool, &victim_owner, &victim.hex).await;

    let signed = sign(
        &attacker,
        CommunityPostDelete {
            community_id: attacker_home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    let env = envelope(&signed, &wallet_addr(&attacker));
    let outcome = consumer::apply_envelope(&state, &env).await;

    assert!(
        !post_is_deleted(&pool, &post_sig).await,
        "an owner of another community must not tombstone this post"
    );
    assert!(
        outcome.is_err(),
        "the cross-community gossip delete must be rejected"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn gossip_post_like_cannot_reach_another_communitys_post() {
    let Some(scratch) = setup_db("cg_social_xcw_glike").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let victim_owner = mk_wallet(147);
    let attacker = mk_wallet(148);

    let victim = found_community(&pool, &victim_owner, "VictimLikesCommunity", true).await;
    let attacker_home = found_community(&pool, &attacker, "AttackerGossipLike", false).await;
    let post_sig = publish_post(&pool, &victim_owner, &victim.hex).await;

    let signed = sign(
        &attacker,
        CommunityPostLike {
            community_id: attacker_home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    let env = envelope(&signed, &wallet_addr(&attacker));
    let outcome = consumer::apply_envelope(&state, &env).await;

    assert_eq!(
        likes_on(&pool, &post_sig).await,
        0,
        "an outsider must not register a like on a private community's post over gossip"
    );
    assert!(
        outcome.is_err(),
        "the cross-community gossip like must be rejected"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn gossip_post_unlike_cannot_reach_another_communitys_like() {
    let Some(scratch) = setup_db("cg_social_xcw_gunlike").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(151);
    let attacker = mk_wallet(152);

    let home = found_community(&pool, &owner, "UnlikeHomeCommunity", false).await;
    let attacker_home = found_community(&pool, &attacker, "AttackerGossipUnlike", false).await;
    let post_sig = publish_post(&pool, &owner, &home.hex).await;

    let liked = sign(
        &owner,
        CommunityPostLike {
            community_id: home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    consumer::apply_envelope(&state, &envelope(&liked, &wallet_addr(&owner)))
        .await
        .expect("the owner's like in their own community must land");
    assert_eq!(active_likes_on(&pool, &post_sig).await, 1);

    let foreign = sign(
        &attacker,
        CommunityPostUnlike {
            community_id: attacker_home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    let outcome =
        consumer::apply_envelope(&state, &envelope(&foreign, &wallet_addr(&attacker))).await;

    assert!(
        outcome.is_err(),
        "an unlike naming a community the post does not belong to must be refused, not silently accepted"
    );
    assert_eq!(
        active_likes_on(&pool, &post_sig).await,
        1,
        "the owner's like must survive a foreign unlike"
    );

    let own = sign(
        &owner,
        CommunityPostUnlike {
            community_id: home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    consumer::apply_envelope(&state, &envelope(&own, &wallet_addr(&owner)))
        .await
        .expect("the owner must still be able to withdraw their own like");
    assert_eq!(
        active_likes_on(&pool, &post_sig).await,
        0,
        "a legitimate unlike in the post's own community must still work"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn gossip_request_status_cannot_reach_another_communitys_request() {
    let Some(scratch) = setup_db("cg_social_xcw_greq").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let victim_owner = mk_wallet(149);
    let attacker = mk_wallet(150);
    let applicant = "0x00000000000000000000000000000000000000ac";

    let victim = found_community(&pool, &victim_owner, "VictimRequestsCommunity", true).await;
    let attacker_home = found_community(&pool, &attacker, "AttackerGossipRequests", true).await;
    let request_id = seed_request(&pool, victim.uuid, applicant, "request_to_join").await;

    let signed = sign(
        &attacker,
        CommunityRequestStatusUpdate {
            community_id: attacker_home.hex.clone(),
            request_id: request_id.to_string(),
            status: "accepted".into(),
        },
        domains::communities(),
    )
    .await;
    let env = envelope(&signed, &wallet_addr(&attacker));
    let outcome = consumer::apply_envelope(&state, &env).await;

    assert_eq!(
        request_status(&pool, request_id).await,
        "pending",
        "a moderator of another community must not rewrite this request over gossip"
    );
    assert!(
        outcome.is_err(),
        "the cross-community gossip request update must be rejected"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn owner_still_updates_requests_and_likes_in_their_own_community() {
    let Some(scratch) = setup_db("cg_social_xcw_happy").await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(151);
    let applicant = "0x00000000000000000000000000000000000000ad";
    let home = found_community(&pool, &owner, "HappyPathCommunity", true).await;
    let request_id = seed_request(&pool, home.uuid, applicant, "request_to_join").await;
    let post_sig = publish_post(&pool, &owner, &home.hex).await;

    let signed = sign(
        &owner,
        CommunityRequestStatusUpdate {
            community_id: home.hex.clone(),
            request_id: request_id.to_string(),
            status: "accepted".into(),
        },
        domains::communities(),
    )
    .await;
    let path = format!("/v1/communities/{}/requests/{}", home.uuid, request_id);
    let headers = signed_headers(&owner, "patch", &path).await;
    let resp = writes::update_request_status(
        State(state.clone()),
        headers,
        Path(writes::PathIdReq {
            id: home.uuid.to_string(),
            request_id: request_id.to_string(),
        }),
        Bytes::from(serde_json::to_vec(&signed).unwrap()),
    )
    .await;
    assert!(
        resp.status().is_success(),
        "the owner's own request update must still succeed, got {}",
        resp.status()
    );
    assert_eq!(request_status(&pool, request_id).await, "accepted");

    let signed = sign(
        &owner,
        CommunityPostLike {
            community_id: home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    let path = format!("/v1/communities/{}/posts/{}/like", home.uuid, post_sig);
    let headers = signed_headers(&owner, "post", &path).await;
    let resp = writes::like_post(
        State(state.clone()),
        headers,
        Path(writes::PathIdPost {
            id: home.uuid.to_string(),
            post_id: post_sig.clone(),
        }),
        Bytes::from(serde_json::to_vec(&signed).unwrap()),
    )
    .await;
    assert!(
        resp.status().is_success(),
        "the owner's own like must still succeed, got {}",
        resp.status()
    );
    assert_eq!(likes_on(&pool, &post_sig).await, 1);

    let signed = sign(
        &owner,
        CommunityPostDelete {
            community_id: home.hex.clone(),
            post_id: post_sig.clone(),
        },
        domains::communities(),
    )
    .await;
    let env = envelope(&signed, &wallet_addr(&owner));
    consumer::apply_envelope(&state, &env)
        .await
        .expect("the owner's own gossip delete must still apply");
    assert!(post_is_deleted(&pool, &post_sig).await);

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}
