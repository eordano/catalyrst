use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::Json;
use ethers_signers::{LocalWallet, Signer};
use rand::Rng;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_communities::content_store::ContentStore;
use catalyrst_communities::fed::replay::Replay;
use catalyrst_communities::handlers::bans::get_bans_v2;
use catalyrst_communities::handlers::communities::{
    get_communities, get_communities_v2, get_community, get_community_v2,
};
use catalyrst_communities::handlers::members::{get_members, get_members_v2};
use catalyrst_communities::handlers::posts::{get_posts, get_posts_v2};
use catalyrst_communities::handlers::requests::{
    get_community_requests_v2, get_member_requests_v2,
};
use catalyrst_communities::ports::bans::BansComponent;
use catalyrst_communities::ports::communities::CommunitiesComponent;
use catalyrst_communities::ports::invites::InvitesComponent;
use catalyrst_communities::ports::members::MembersComponent;
use catalyrst_communities::ports::moderation::ModerationComponent;
use catalyrst_communities::ports::places::PlacesComponent;
use catalyrst_communities::ports::places_api::PlacesApiClient;
use catalyrst_communities::ports::posts::PostsComponent;
use catalyrst_communities::ports::profiles::ProfilesComponent;
use catalyrst_communities::ports::requests::RequestsComponent;
use catalyrst_communities::ports::voice::VoiceComponent;
use catalyrst_communities::{AppState, AppStateInner};
use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};

fn pg_url() -> Option<String> {
    std::env::var("CATALYRST_COMMUNITIES_TEST_PG")
        .ok()
        .or_else(|| Some("postgres://postgres:postgres@127.0.0.1:5432/communities".into()))
}

fn unique_schema() -> String {
    let mut b = [0u8; 8];
    rand::rng().fill_bytes(&mut b);
    format!("test_v2_{}", hex::encode(b))
}

fn unique_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rnd = [0u8; 8];
    rand::rng().fill_bytes(&mut rnd);
    p.push(format!("cmm-v2-{}-{}", tag, hex::encode(rnd)));
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
    .bind("V2 Test Community")
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

async fn seed_post(pool: &PgPool, id: Uuid, author: &str) {
    sqlx::query(
        "INSERT INTO community_posts (id, community_id, author_address, content) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(rand_uuid())
    .bind(id)
    .bind(author)
    .bind("hello world")
    .execute(pool)
    .await
    .expect("seed post");
}

async fn seed_request(pool: &PgPool, id: Uuid, member: &str) {
    sqlx::query(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1, $2, $3, 'pending', 'request_to_join')",
    )
    .bind(rand_uuid())
    .bind(id)
    .bind(member)
    .execute(pool)
    .await
    .expect("seed request");
}

fn body<E>(r: Result<Json<Value>, E>) -> Value {
    match r {
        Ok(j) => j.0,
        Err(_) => panic!("v2 handler unexpectedly returned Err (should be Ok / no 500)"),
    }
}

fn assert_absent(row: &Value, keys: &[&str], ctx: &str) {
    let m = row
        .as_object()
        .unwrap_or_else(|| panic!("{ctx}: row not object"));
    for k in keys {
        assert!(
            !m.contains_key(*k),
            "{ctx}: v2 row must be address-only but carries profile key `{k}`: {row}"
        );
    }
}

fn assert_no_member_profile(row: &Value, ctx: &str) {
    assert_absent(row, &["name", "profilePictureUrl", "hasClaimedName"], ctx);
}

fn assert_no_author_profile(row: &Value, ctx: &str) {
    assert_absent(
        row,
        &[
            "authorName",
            "authorProfilePictureUrl",
            "authorHasClaimedName",
        ],
        ctx,
    );
}

fn assert_no_owner_name(row: &Value, ctx: &str) {
    assert_absent(row, &["ownerName"], ctx);
}

fn mk_wallet(seed: u8) -> LocalWallet {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    LocalWallet::from_bytes(&key).expect("wallet from bytes")
}

fn wallet_addr(w: &LocalWallet) -> String {
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

async fn signed_headers(wallet: &LocalWallet, path: &str) -> HeaderMap {
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
    let canonical = format!("{}:{}:{}:{}", "get", path, ts_ms, "{}").to_lowercase();
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
            &format!("0x{}", ephemeral_sig),
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&link_json(
            "ECDSA_SIGNED_ENTITY",
            &canonical,
            &format!("0x{}", entity_sig),
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
async fn v2_members_are_address_only_and_never_drop_unresolved() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_members_are_address_only_and_never_drop_unresolved: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let cid = rand_uuid();
    let owner = "0x0000000000000000000000000000000000000001";
    seed_community(&pool, cid, owner, false).await;
    seed_member(&pool, cid, owner, "owner").await;
    seed_member(
        &pool,
        cid,
        "0x0000000000000000000000000000000000000002",
        "member",
    )
    .await;
    seed_member(
        &pool,
        cid,
        "0x000000000000000000000000000000000000dead",
        "member",
    )
    .await;

    let v2 = body(
        get_members_v2(
            State(state.clone()),
            HeaderMap::new(),
            Path(cid.to_string()),
            Query(Vec::new()),
        )
        .await,
    );
    let results = v2["data"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3, "v2 must not drop unresolved members");
    assert_eq!(v2["data"]["total"], 3);

    let mut saw_dead = false;
    for row in results {
        assert_no_member_profile(row, "members_v2");
        let m = row.as_object().unwrap();
        assert!(m.contains_key("memberAddress"), "member keeps address");
        assert!(m.contains_key("role"));
        assert!(m.contains_key("friendshipStatus"));
        if m["memberAddress"] == "0x000000000000000000000000000000000000dead" {
            saw_dead = true;
        }
    }
    assert!(
        saw_dead,
        "the no-profile member must be present, not dropped"
    );

    let v1 = body(
        get_members(
            State(state.clone()),
            HeaderMap::new(),
            Path(cid.to_string()),
            Query(Vec::new()),
        )
        .await,
    );
    let v1_rows = v1["data"]["results"].as_array().unwrap();
    assert_eq!(
        v1_rows.len(),
        3,
        "rust v1 placeholders (does not drop) either"
    );
    for row in v1_rows {
        let m = row.as_object().unwrap();
        assert!(
            m.contains_key("name") && m.contains_key("hasClaimedName"),
            "v1 must carry (placeholder) profile keys that v2 omits"
        );
    }

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v2_single_community_is_address_only_and_no_500() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_single_community_is_address_only_and_no_500: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let cid = rand_uuid();
    let owner = "0x00000000000000000000000000000000000000aa";
    seed_community(&pool, cid, owner, false).await;

    let v2 = body(
        get_community_v2(
            State(state.clone()),
            HeaderMap::new(),
            Path(cid.to_string()),
        )
        .await,
    );
    let data = &v2["data"];
    assert_no_owner_name(data, "community_v2");
    assert_eq!(data["ownerAddress"], owner, "address is preserved");
    assert_eq!(data["thumbnailUrl"], "N/A");
    assert!(
        data.as_object().unwrap().get("_hasThumbnail").is_none(),
        "internal _hasThumbnail must be stripped"
    );

    let v1 = body(
        get_community(
            State(state.clone()),
            HeaderMap::new(),
            Path(cid.to_string()),
        )
        .await,
    );
    assert!(
        v1["data"].as_object().unwrap().contains_key("ownerName"),
        "v1 adds ownerName; v2 must not"
    );

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v2_communities_list_is_address_only() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_communities_list_is_address_only: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let cid = rand_uuid();
    let owner = "0x00000000000000000000000000000000000000bb";
    seed_community(&pool, cid, owner, false).await;

    let v2 =
        body(get_communities_v2(State(state.clone()), HeaderMap::new(), Query(Vec::new())).await);
    let results = v2["data"]["results"].as_array().expect("results");
    assert!(
        !results.is_empty(),
        "seeded public community must be listed"
    );
    for row in results {
        assert_no_owner_name(row, "communities_v2");
        let m = row.as_object().unwrap();
        assert!(m.contains_key("ownerAddress"));
        assert!(m.contains_key("thumbnailUrl"));
        assert!(m.get("_hasThumbnail").is_none());
    }

    let v1 = body(get_communities(State(state.clone()), HeaderMap::new(), Query(Vec::new())).await);
    for row in v1["data"]["results"].as_array().unwrap() {
        assert!(
            row.as_object().unwrap().contains_key("ownerName"),
            "v1 list carries ownerName; v2 must not"
        );
    }

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v2_posts_are_address_only() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_posts_are_address_only: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let cid = rand_uuid();
    let owner = "0x00000000000000000000000000000000000000cc";
    seed_community(&pool, cid, owner, false).await;
    seed_post(&pool, cid, "0x00000000000000000000000000000000000000c1").await;

    let v2 = body(
        get_posts_v2(
            State(state.clone()),
            HeaderMap::new(),
            Path(cid.to_string()),
            Query(Vec::new()),
        )
        .await,
    );
    let posts = v2["data"]["posts"].as_array().expect("posts array");
    assert_eq!(posts.len(), 1);
    for row in posts {
        assert_no_author_profile(row, "posts_v2");
        let m = row.as_object().unwrap();
        assert!(m.contains_key("authorAddress"), "post keeps author address");
        assert!(m.contains_key("content"));
    }

    let v1 = body(
        get_posts(
            State(state.clone()),
            HeaderMap::new(),
            Path(cid.to_string()),
            Query(Vec::new()),
        )
        .await,
    );
    for row in v1["data"]["posts"].as_array().unwrap() {
        assert!(
            row.as_object().unwrap().contains_key("authorName"),
            "v1 posts carry authorName; v2 must not"
        );
    }

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v2_bans_are_address_only() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_bans_are_address_only: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let owner_wallet = mk_wallet(71);
    let owner = wallet_addr(&owner_wallet);

    let cid = rand_uuid();
    seed_community(&pool, cid, &owner, false).await;
    seed_member(&pool, cid, &owner, "owner").await;
    seed_ban(
        &pool,
        cid,
        "0x000000000000000000000000000000000000dea1",
        &owner,
    )
    .await;

    let path = format!("/v2/communities/{}/bans", cid);
    let headers = signed_headers(&owner_wallet, &path).await;

    let v2 = body(
        get_bans_v2(
            State(state.clone()),
            headers,
            Path(cid.to_string()),
            Query(Vec::new()),
        )
        .await,
    );
    let results = v2["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 1, "banned member not dropped");
    for row in results {
        assert_no_member_profile(row, "bans_v2");
        let m = row.as_object().unwrap();
        assert!(m.contains_key("memberAddress"), "ban keeps banned address");
        assert!(m.contains_key("friendshipStatus"));
    }

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v2_community_requests_are_address_only() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_community_requests_are_address_only: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let owner_wallet = mk_wallet(72);
    let owner = wallet_addr(&owner_wallet);

    let cid = rand_uuid();
    seed_community(&pool, cid, &owner, false).await;
    seed_member(&pool, cid, &owner, "owner").await;
    seed_request(&pool, cid, "0x000000000000000000000000000000000000dea2").await;

    let path = format!("/v2/communities/{}/requests", cid);
    let headers = signed_headers(&owner_wallet, &path).await;

    let v2 = body(
        get_community_requests_v2(
            State(state.clone()),
            headers,
            Path(cid.to_string()),
            Query(Vec::new()),
        )
        .await,
    );
    let results = v2["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 1, "request not dropped");
    for row in results {
        assert_no_member_profile(row, "community_requests_v2");
        let m = row.as_object().unwrap();
        assert!(
            m.contains_key("memberAddress"),
            "request keeps member address"
        );
        assert!(m.contains_key("friendshipStatus"));
    }

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn v2_member_requests_omit_owner_name() {
    let Some((pool, schema, admin_url)) = setup_db().await else {
        eprintln!("skipping v2_member_requests_omit_owner_name: no postgres");
        return;
    };
    let (state, dir) = build_state(&pool).await;

    let member_wallet = mk_wallet(73);
    let member = wallet_addr(&member_wallet);

    let cid = rand_uuid();
    let owner = "0x00000000000000000000000000000000000000ee";
    seed_community(&pool, cid, owner, false).await;
    seed_request(&pool, cid, &member).await;

    let path = format!("/v2/members/{}/requests", member);
    let headers = signed_headers(&member_wallet, &path).await;

    let v2 = body(
        get_member_requests_v2(
            State(state.clone()),
            headers,
            Path(member.clone()),
            Query(Vec::new()),
        )
        .await,
    );
    let results = v2["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 1);
    for row in results {
        assert_no_owner_name(row, "member_requests_v2");
        let m = row.as_object().unwrap();
        assert!(
            m.contains_key("ownerAddress"),
            "keeps community owner address"
        );
        assert!(m.contains_key("communityId"));
        assert!(m.contains_key("thumbnailUrl"));
        assert!(m.get("_hasThumbnail").is_none(), "internal flag stripped");
    }

    cleanup(&admin_url, &schema).await;
    let _ = std::fs::remove_dir_all(&dir);
}
