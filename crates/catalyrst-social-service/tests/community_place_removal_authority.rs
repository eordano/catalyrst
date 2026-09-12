//! `rest::handlers::client::places::remove_place` gates on `Permission::RemovePlaces` ahead
//! of the place-ownership probe, so a caller with no standing in a private community gets
//! one answer whether or not the named place is attached -- the place list `get_places`
//! refuses to non-members stays closed. Upstream social-service-ea#496, alongside the
//! create-request ordering pinned in `tests/join_and_ban_integrity.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use catalyrst_contract_gate::pg::ScratchSchema;
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_fed::sig::domains;
use catalyrst_fed::{NoopPublisher, RateLimiter};
use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
use catalyrst_social_service::rest::handlers::client;
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
    p.push(format!("cmm-placeauth-{}-{}", tag, hex::encode(rnd)));
    p
}

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create_or_default(
        "CATALYRST_SOCIAL_SERVICE_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/communities",
        "cg_social_placeauth",
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
    .bind("Place Authority Community")
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

async fn attach_place(pool: &PgPool, community: Uuid, place: &str, added_by: &str) {
    sqlx::query(
        "INSERT INTO community_places (id, community_id, added_by, added_at) \
         VALUES ($1, $2, $3, now())",
    )
    .bind(place)
    .bind(community)
    .bind(added_by)
    .execute(pool)
    .await
    .expect("attach place");
}

async fn place_is_attached(pool: &PgPool, community: Uuid, place: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM community_places WHERE community_id = $1 AND id = $2",
    )
    .bind(community)
    .bind(place)
    .fetch_one(pool)
    .await
    .expect("place probe")
        > 0
}

#[tokio::test]
async fn a_caller_without_remove_places_learns_nothing_about_the_place_list() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let owner = "0x0000000000000000000000000000000000000001";
    let outsider = mk_wallet(130);
    let outsider_addr = wallet_addr(&outsider);

    let cid = rand_uuid();
    seed_community(&pool, cid, owner, true).await;
    seed_member(&pool, cid, owner, "owner").await;

    let attached = "bafkreiattachedplacefixture";
    let absent = "bafkreiabsentplacefixture";
    attach_place(&pool, cid, attached, owner).await;

    let mut answers: Vec<(StatusCode, String)> = Vec::new();
    for place in [attached, absent] {
        let headers = signed_headers(
            &outsider,
            "delete",
            &format!("/v1/communities/{}/places/{}", cid, place),
        )
        .await;
        let resp = client::remove_place(
            State(state.clone()),
            headers,
            Path(client::PathIdPlace {
                id: cid.to_string(),
                place_id: place.to_string(),
            }),
        )
        .await;
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .expect("refusal body");
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "a caller without remove_places must be refused for {}",
            place
        );
        answers.push((status, String::from_utf8_lossy(&body).to_string()));
    }

    assert!(
        answers[0].1.contains(&format!(
            "The user {} doesn't have permission to remove places from the community",
            outsider_addr
        )),
        "the refusal must name the missing permission, got {}",
        answers[0].1
    );
    assert_eq!(
        answers[0], answers[1],
        "an attached place and one the community does not hold must be indistinguishable"
    );
    assert!(
        place_is_attached(&pool, cid, attached).await,
        "the refused removal must leave the place attached"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_owner_still_removes_a_place_through_the_permission_gate() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(131);
    let owner_addr = wallet_addr(&owner);

    let cid = rand_uuid();
    seed_community(&pool, cid, &owner_addr, true).await;
    seed_member(&pool, cid, &owner_addr, "owner").await;

    let place = "bafkreiownedplacefixture";
    attach_place(&pool, cid, place, &owner_addr).await;

    let headers = signed_headers(
        &owner,
        "delete",
        &format!("/v1/communities/{}/places/{}", cid, place),
    )
    .await;
    let resp = client::remove_place(
        State(state.clone()),
        headers,
        Path(client::PathIdPlace {
            id: cid.to_string(),
            place_id: place.to_string(),
        }),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "an owner holds remove_places and needs no special case"
    );
    assert!(
        !place_is_attached(&pool, cid, place).await,
        "the owner's removal must detach the place"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_owner_removing_an_unattached_place_gets_a_not_found() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let (state, dir) = build_state(&pool).await;

    let owner = mk_wallet(132);
    let owner_addr = wallet_addr(&owner);
    let outsider = mk_wallet(133);
    let outsider_addr = wallet_addr(&outsider);

    let cid = rand_uuid();
    seed_community(&pool, cid, &owner_addr, true).await;
    seed_member(&pool, cid, &owner_addr, "owner").await;

    let absent = "bafkreiunattachedplacefixture";
    let headers = signed_headers(
        &owner,
        "delete",
        &format!("/v1/communities/{}/places/{}", cid, absent),
    )
    .await;
    let resp = client::remove_place(
        State(state.clone()),
        headers,
        Path(client::PathIdPlace {
            id: cid.to_string(),
            place_id: absent.to_string(),
        }),
    )
    .await;
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .expect("not-found body");
    let body = String::from_utf8_lossy(&body).to_string();
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an authorized caller must learn the place is not held, got {}",
        body
    );
    assert!(
        body.contains(&format!("Place {} not found in community {}", absent, cid)),
        "the 404 must name the place and the community, got {}",
        body
    );

    let attached = "bafkreiattachedplacefixture";
    attach_place(&pool, cid, attached, &owner_addr).await;
    let mut refusals: Vec<(StatusCode, String)> = Vec::new();
    for place in [attached, absent] {
        let headers = signed_headers(
            &outsider,
            "delete",
            &format!("/v1/communities/{}/places/{}", cid, place),
        )
        .await;
        let resp = client::remove_place(
            State(state.clone()),
            headers,
            Path(client::PathIdPlace {
                id: cid.to_string(),
                place_id: place.to_string(),
            }),
        )
        .await;
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .expect("refusal body");
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "the 404 must stay strictly behind the permission gate for {}",
            place
        );
        refusals.push((status, String::from_utf8_lossy(&body).to_string()));
    }
    assert!(
        refusals[0].1.contains(&format!(
            "The user {} doesn't have permission to remove places from the community",
            outsider_addr
        )),
        "the refusal must name the missing permission, got {}",
        refusals[0].1
    );
    assert_eq!(
        refusals[0], refusals[1],
        "restoring the authorized 404 must not reopen the place-list differential"
    );
    assert!(
        place_is_attached(&pool, cid, attached).await,
        "the refused removal must leave the place attached"
    );

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&dir);
}
