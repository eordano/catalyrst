use std::sync::Arc;
use std::time::Duration;

use catalyrst_contract_gate::pg::ScratchSchema;
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::handlers::members::get_members;
use catalyrst_social_service::rest::http::Pagination;
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
    let scratch = ScratchSchema::create_or_default(
        "CATALYRST_SOCIAL_SERVICE_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/communities",
        "cg_social_online",
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
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0003_voice_moderators.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0004_thumbnail_hash.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0005_suspension.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0006_role_check_reconcile.sql"),
    )
    .await;

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
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn rand_uuid() -> Uuid {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    Uuid::from_bytes(b)
}

async fn seed_community(pool: &PgPool, id: Uuid, owner: &str) {
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted) \
         VALUES ($1, $2, $3, $4, FALSE, TRUE, FALSE)",
    )
    .bind(id)
    .bind("Online Test Community")
    .bind("description")
    .bind(owner)
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

fn page(limit: i64, offset: i64) -> Pagination {
    Pagination { limit, offset }
}

#[tokio::test]
async fn list_online_filters_to_the_supplied_online_set() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let cid = rand_uuid();
    let owner = "0x0000000000000000000000000000000000000001";
    let m2 = "0x0000000000000000000000000000000000000002";
    let m3 = "0x0000000000000000000000000000000000000003";
    seed_community(&pool, cid, owner).await;
    seed_member(&pool, cid, owner, "owner").await;
    seed_member(&pool, cid, m2, "member").await;
    seed_member(&pool, cid, m3, "member").await;

    let members = MembersComponent::new(pool.clone());

    let (all, all_total) = members.list(cid, &page(10, 0)).await.expect("list");
    assert_eq!(all.len(), 3);
    assert_eq!(all_total, 3);

    let online = vec![owner.to_string(), m3.to_string()];
    let (online_members, online_total) = members
        .list_online(cid, &online, &page(10, 0))
        .await
        .expect("list_online");
    assert_eq!(online_total, 2, "total counts only online members");
    let addrs: Vec<&str> = online_members
        .iter()
        .map(|m| m.member_address.as_str())
        .collect();
    assert_eq!(online_members.len(), 2);
    assert!(addrs.contains(&owner), "owner (online) present");
    assert!(addrs.contains(&m3), "m3 (online) present");
    assert!(!addrs.contains(&m2), "m2 (offline) excluded");
    assert_eq!(online_members[0].member_address, owner);

    scratch.drop().await;
}

#[tokio::test]
async fn list_online_is_case_insensitive_on_addresses() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let cid = rand_uuid();
    let owner = "0x0000000000000000000000000000000000000001";
    let member = "0x00000000000000000000000000000000000000ab";
    seed_community(&pool, cid, owner).await;
    seed_member(&pool, cid, member, "member").await;

    let members = MembersComponent::new(pool.clone());

    let online = vec!["0x00000000000000000000000000000000000000AB".to_string()];
    let (rows, total) = members
        .list_online(cid, &online, &page(10, 0))
        .await
        .expect("list_online");
    assert_eq!(total, 1, "mixed-case online address still matches");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].member_address, member);

    scratch.drop().await;
}

async fn build_state(pool: PgPool, peers_stats: PeersStatsClient) -> AppState {
    let content_dir = std::env::temp_dir().join(format!("online-members-{}", rand_uuid()));
    let content_store = Arc::new(ContentStore::new(content_dir, MAX_BODY_BYTES));
    content_store.init().await.unwrap();
    Arc::new(AppStateInner {
        admin_token: None,
        bans: BansComponent::new(pool.clone()),
        communities: CommunitiesComponent::new(pool.clone()),
        invites: InvitesComponent::new(pool.clone()),
        members: MembersComponent::new(pool.clone()),
        moderation: ModerationComponent::new(pool.clone()),
        peers_stats,
        places: PlacesComponent::new(pool.clone()),
        places_api: PlacesApiClient::new(None),
        posts: PostsComponent::new(pool.clone()),
        profiles: Arc::new(ProfilesComponent::new(None, "http://127.0.0.1:9".into())),
        requests: RequestsComponent::new(pool.clone()),
        voice: VoiceComponent::new(pool.clone()),
        pool: pool.clone(),
        mutes_pool: None,
        replay: Replay::new(pool.clone()).await.unwrap(),
        limiter: Arc::new(catalyrst_fed::RateLimiter::new(
            10_000,
            Duration::from_secs(60),
        )),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::communities(),
        content_store,
        cdn_url: "http://cdn.test".into(),
        global_moderators: vec![],
        restricted_names: vec![],
        gatekeeper: Gatekeeper::with_token("http://127.0.0.1:1".to_string(), None),
    })
}

/// Serves a canned archipelago-stats `/peers` body on an ephemeral port.
async fn mock_peers_source(body: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route("/peers", axum::routing::get(move || async move { body }));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

async fn call_only_online(state: AppState, cid: Uuid) -> serde_json::Value {
    let resp = get_members(
        axum::extract::State(state),
        axum::http::HeaderMap::new(),
        axum::extract::Path(cid.to_string()),
        axum::extract::Query(vec![("onlyOnline".to_string(), "true".to_string())]),
    )
    .await
    .expect("onlyOnline listing succeeds");
    resp.0
}

#[tokio::test]
async fn only_online_reflects_the_presence_source() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let cid = rand_uuid();
    let owner = "0x0000000000000000000000000000000000000001";
    let m2 = "0x0000000000000000000000000000000000000002";
    let m3 = "0x00000000000000000000000000000000000000ab";
    seed_community(&pool, cid, owner).await;
    seed_member(&pool, cid, owner, "owner").await;
    seed_member(&pool, cid, m2, "member").await;
    seed_member(&pool, cid, m3, "member").await;

    // Mixed-case ids plus a non-member peer: the handler lowercases and the
    // membership filter drops peers that are not in this community.
    let base = mock_peers_source(
        r#"{"ok":true,"peers":[
            {"id":"0x0000000000000000000000000000000000000001"},
            {"id":"0x00000000000000000000000000000000000000AB"},
            {"id":"0xffffffffffffffffffffffffffffffffffffffff"}
        ]}"#,
    )
    .await;
    let state = build_state(pool.clone(), PeersStatsClient::new(base)).await;

    let body = call_only_online(state, cid).await;
    let data = &body["data"];
    assert_eq!(
        data["total"], 2,
        "presence source yields two online members"
    );
    let addrs: Vec<&str> = data["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["memberAddress"].as_str().unwrap())
        .collect();
    assert_eq!(
        addrs,
        vec![owner, m3],
        "owner sorts first, offline m2 absent"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn only_online_fails_open_to_empty_when_presence_source_is_down() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let cid = rand_uuid();
    let owner = "0x0000000000000000000000000000000000000001";
    seed_community(&pool, cid, owner).await;
    seed_member(&pool, cid, owner, "owner").await;

    let state = build_state(
        pool.clone(),
        PeersStatsClient::new("http://127.0.0.1:1".to_string()),
    )
    .await;

    let body = call_only_online(state, cid).await;
    let data = &body["data"];
    assert_eq!(
        data["total"], 0,
        "unreachable presence source means offline"
    );
    assert!(data["results"].as_array().unwrap().is_empty());

    scratch.drop().await;
}

#[tokio::test]
async fn list_online_empty_set_returns_empty() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let cid = rand_uuid();
    let owner = "0x0000000000000000000000000000000000000001";
    seed_community(&pool, cid, owner).await;
    seed_member(&pool, cid, owner, "owner").await;
    seed_member(
        &pool,
        cid,
        "0x0000000000000000000000000000000000000002",
        "member",
    )
    .await;

    let members = MembersComponent::new(pool.clone());

    let (rows, total) = members
        .list_online(cid, &[], &page(10, 0))
        .await
        .expect("list_online empty");
    assert!(rows.is_empty(), "empty online set yields no rows");
    assert_eq!(total, 0, "empty online set yields total 0");

    scratch.drop().await;
}
