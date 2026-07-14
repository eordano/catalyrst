use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{
    multipart_body, signed_fetch_headers, test_wallet, Case, Gate, MultipartPart, Wallet,
};
use serde_json::json;
use sqlx::PgPool;

use catalyrst_social_service::gatekeeper::Gatekeeper;
use catalyrst_social_service::rest::api_router_with_spec;
use catalyrst_social_service::rest::content_store::{ContentStore, MAX_BODY_BYTES};
use catalyrst_social_service::rest::fed::replay::Replay;
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

const ADMIN_TOKEN: &str = "cg-social-admin";
const C1: &str = "11111111-1111-1111-1111-111111111111";
const C_DEL: &str = "22222222-2222-2222-2222-222222222222";
const C_PRIV: &str = "33333333-3333-3333-3333-333333333333";
const P1: &str = "aaaaaaaa-1111-1111-1111-111111111111";
const P2: &str = "aaaaaaaa-2222-2222-2222-222222222222";
const R1: &str = "bbbbbbbb-1111-1111-1111-111111111111";
const F1: &str = "cccccccc-1111-1111-1111-111111111111";
const FA1: &str = "cccccccc-2222-2222-2222-222222222222";
const MISSING: &str = "99999999-9999-9999-9999-999999999999";
const FORGED: &str = "/v1/forged/not-a-route";

fn addr(w: &Wallet) -> String {
    w.address().to_lowercase()
}

fn with_headers(mut c: Case, hs: Vec<(String, String)>) -> Case {
    for (k, v) in hs {
        c = c.header(&k, &v);
    }
    c
}

fn forged(method: &str, spec: &str, actual: &str, wallet: &Wallet) -> Case {
    let hs = signed_fetch_headers(wallet, method, FORGED);
    with_headers(Case::new(method, spec).path(actual).expect(401), hs)
}

async fn build_state(pool: PgPool, content_dir: std::path::PathBuf, moderator: String) -> AppState {
    let content_store = Arc::new(ContentStore::new(content_dir, MAX_BODY_BYTES));
    content_store.init().await.unwrap();
    Arc::new(AppStateInner {
        admin_token: Some(ADMIN_TOKEN.into()),
        bans: BansComponent::new(pool.clone()),
        communities: CommunitiesComponent::new(pool.clone()),
        invites: InvitesComponent::new(pool.clone()),
        members: MembersComponent::new(pool.clone()),
        moderation: ModerationComponent::new(pool.clone()),
        peers_stats: PeersStatsClient::new("http://127.0.0.1:1".to_string()),
        places: PlacesComponent::new(pool.clone()),
        places_api: PlacesApiClient::new(None),
        posts: PostsComponent::new(pool.clone()),
        profiles: Arc::new(ProfilesComponent::new(None, "http://127.0.0.1:9".into())),
        requests: RequestsComponent::new(pool.clone()),
        voice: VoiceComponent::new(pool.clone()),
        pool: pool.clone(),
        mutes_pool: Some(pool.clone()),
        replay: Replay::new(pool.clone()).await.unwrap(),
        limiter: Arc::new(catalyrst_fed::RateLimiter::new(
            10_000,
            Duration::from_secs(60),
        )),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::communities(),
        content_store,
        cdn_url: "http://cdn.test".into(),
        global_moderators: vec![moderator],
        restricted_names: vec![],
        gatekeeper: Gatekeeper::with_token("http://127.0.0.1:1".to_string(), None),
    })
}

async fn seed(pool: &PgPool, user: &str, m1: &str, m2: &str, m3: &str, friend: &str) {
    for (id, private) in [(C1, false), (C_DEL, false), (C_PRIV, true)] {
        sqlx::query(
            "INSERT INTO communities (id, name, description, owner_address, private, active) \
             VALUES ($1::uuid, $2, 'seeded community', $3, $4, TRUE)",
        )
        .bind(id)
        .bind(format!("community {}", &id[..8]))
        .bind(user)
        .bind(private)
        .execute(pool)
        .await
        .unwrap();
    }

    for (community, member, role) in [
        (C1, user, "owner"),
        (C1, m1, "member"),
        (C1, m2, "member"),
        (C_DEL, user, "owner"),
        (C_PRIV, user, "owner"),
    ] {
        sqlx::query(
            "INSERT INTO community_members (community_id, member_address, role) \
             VALUES ($1::uuid, $2, $3)",
        )
        .bind(community)
        .bind(member)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }

    for post in [P1, P2] {
        sqlx::query(
            "INSERT INTO community_posts (id, community_id, author_address, content) \
             VALUES ($1::uuid, $2::uuid, $3, 'seeded post')",
        )
        .bind(post)
        .bind(C1)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO community_requests (id, community_id, member_address, status, type) \
         VALUES ($1::uuid, $2::uuid, $3, 'pending', 'request_to_join')",
    )
    .bind(R1)
    .bind(C_PRIV)
    .bind(m3)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO friendships (id, address_requester, address_requested, is_active) \
         VALUES ($1::uuid, $2, $3, TRUE)",
    )
    .bind(F1)
    .bind(user)
    .bind(friend)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO friendship_actions (id, friendship_id, action, acting_user) \
         VALUES ($1::uuid, $2::uuid, 'accept', $3)",
    )
    .bind(FA1)
    .bind(F1)
    .bind(friend)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn every_spec_route_answers_its_contract() {
    let Some(scratch) = ScratchDb::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social").await
    else {
        return;
    };
    sqlx::migrate!("./migrations")
        .run(&scratch.pool)
        .await
        .unwrap();

    let user = test_wallet(7);
    let modw = test_wallet(9);
    let joiner = test_wallet(11);
    let m1 = test_wallet(13);
    let m2 = test_wallet(15);
    let m3 = test_wallet(17);
    let req_user = test_wallet(19);
    let friend = test_wallet(23);

    seed(
        &scratch.pool,
        &addr(&user),
        &addr(&m1),
        &addr(&m2),
        &addr(&m3),
        &addr(&friend),
    )
    .await;

    let content_dir = std::env::temp_dir().join(format!("cg-social-{}", scratch.database));
    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone(), content_dir.clone(), addr(&modw)).await;

    let thumb = vec![137u8, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3, 4];
    let thumb_hash = state.content_store.put(&thumb).await.unwrap();
    sqlx::query(
        "INSERT INTO community_ranking_metrics (community_id, has_thumbnail, thumbnail_hash) \
         VALUES ($1::uuid, TRUE, $2)",
    )
    .bind(C1)
    .bind(&thumb_hash)
    .execute(&scratch.pool)
    .await
    .unwrap();

    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    read_surface(&mut gate, &app, &user, &modw, &friend).await;
    write_surface(
        &mut gate, &app, &user, &joiner, &m1, &m2, &req_user, &friend,
    )
    .await;
    admin_and_federation(&mut gate, &app, &user, &thumb_hash).await;

    gate.assert_covered();

    scratch.drop().await;
    let _ = std::fs::remove_dir_all(&content_dir);
}

async fn read_surface(
    gate: &mut Gate,
    app: &Router,
    user: &Wallet,
    modw: &Wallet,
    friend: &Wallet,
) {
    let c1 = format!("/v1/communities/{}", C1);
    let missing = format!("/v1/communities/{}", MISSING);

    for signed in [false, true] {
        let mut c = Case::new("get", "/v1/communities");
        if signed {
            c = c.signed(user);
        }
        gate.hit(app, c).await;
        let mut c = Case::new("get", "/v2/communities");
        if signed {
            c = c.signed(user);
        }
        gate.hit(app, c).await;
    }

    gate.hit(
        app,
        Case::new("get", "/v1/communities/{id}")
            .path(&c1)
            .signed(user),
    )
    .await;
    gate.hit(app, Case::new("get", "/v1/communities/{id}").path(&c1))
        .await;
    gate.hit(
        app,
        Case::new("get", "/v1/communities/{id}")
            .path(&missing)
            .expect(404),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v2/communities/{id}").path(&format!("/v2/communities/{}", C1)),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v2/communities/{id}")
            .path(&format!("/v2/communities/{}", MISSING))
            .expect(404),
    )
    .await;

    for (spec, base) in [
        ("/v1/communities/{id}/members", "/v1/communities"),
        ("/v2/communities/{id}/members", "/v2/communities"),
        ("/v1/communities/{id}/places", "/v1/communities"),
        ("/v1/communities/{id}/posts", "/v1/communities"),
        ("/v2/communities/{id}/posts", "/v2/communities"),
    ] {
        let seg = spec.rsplit('/').next().unwrap();
        gate.hit(
            app,
            Case::new("get", spec)
                .path(&format!("{}/{}/{}", base, C1, seg))
                .signed(user),
        )
        .await;
        gate.hit(
            app,
            Case::new("get", spec).path(&format!("{}/{}/{}", base, C1, seg)),
        )
        .await;
        gate.hit(
            app,
            Case::new("get", spec)
                .path(&format!("{}/{}/{}", base, MISSING, seg))
                .expect(404),
        )
        .await;
    }

    for (spec, base) in [
        ("/v1/communities/{id}/bans", "/v1/communities"),
        ("/v2/communities/{id}/bans", "/v2/communities"),
        ("/v1/communities/{id}/requests", "/v1/communities"),
        ("/v2/communities/{id}/requests", "/v2/communities"),
    ] {
        let seg = spec.rsplit('/').next().unwrap();
        let actual = format!("{}/{}/{}", base, C1, seg);
        gate.hit(app, Case::new("get", spec).path(&actual).signed(user))
            .await;
        gate.hit(app, Case::new("get", spec).path(&actual).expect(401))
            .await;
        gate.hit(app, forged("get", spec, &actual, user)).await;
    }

    gate.hit(
        app,
        Case::new("get", "/v1/members/{address}/communities")
            .path(&format!("/v1/members/{}/communities", addr(user)))
            .signed(user),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v1/members/{address}/communities")
            .path(&format!("/v1/members/{}/communities", addr(user))),
    )
    .await;

    for spec in [
        "/v1/members/{address}/requests",
        "/v2/members/{address}/requests",
    ] {
        let base = spec.trim_end_matches("/requests");
        let actual = format!("{}/requests", base.replace("{address}", &addr(user)));
        gate.hit(app, Case::new("get", spec).path(&actual).signed(user))
            .await;
        gate.hit(app, Case::new("get", spec).path(&actual).expect(401))
            .await;
        gate.hit(app, forged("get", spec, &actual, user)).await;
    }

    let invites = format!("/v1/members/{}/invites", addr(modw));
    gate.hit(
        app,
        Case::new("get", "/v1/members/{address}/invites")
            .path(&invites)
            .signed(user),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v1/members/{address}/invites")
            .path(&invites)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged("get", "/v1/members/{address}/invites", &invites, user),
    )
    .await;

    let managed = format!("/v1/communities/{}/managed", addr(user));
    gate.hit(
        app,
        Case::new("get", "/v1/communities/{address}/managed")
            .path(&managed)
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v1/communities/{address}/managed")
            .path(&managed)
            .expect(401),
    )
    .await;

    gate.hit(app, Case::new("get", "/v1/mutes").signed(user))
        .await;
    gate.hit(app, Case::new("get", "/v1/mutes").expect(401))
        .await;
    gate.hit(app, forged("get", "/v1/mutes", "/v1/mutes", user))
        .await;

    gate.hit(app, Case::new("get", "/v1/friends").signed(user))
        .await;
    gate.hit(app, Case::new("get", "/v1/friends").expect(401))
        .await;
    gate.hit(app, forged("get", "/v1/friends", "/v1/friends", user))
        .await;

    let messages = format!("/v1/friends/{}/messages", addr(friend));
    gate.hit(
        app,
        Case::new("get", "/v1/friends/{peer}/messages")
            .path(&messages)
            .signed(user),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v1/friends/{peer}/messages")
            .path(&messages)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged("get", "/v1/friends/{peer}/messages", &messages, user),
    )
    .await;

    gate.hit(
        app,
        Case::new("get", "/v1/community-voice-chats/active").signed(user),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v1/community-voice-chats/active").expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "get",
            "/v1/community-voice-chats/active",
            "/v1/community-voice-chats/active",
            user,
        ),
    )
    .await;

    gate.hit(
        app,
        Case::new("get", "/v1/moderation/communities").signed(modw),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/v1/moderation/communities").expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "get",
            "/v1/moderation/communities",
            "/v1/moderation/communities",
            modw,
        ),
    )
    .await;

    let thumb = format!("/social/communities/{}/raw-thumbnail.png", C1);
    gate.hit(
        app,
        Case::new("get", "/social/communities/{id}/raw-thumbnail.png").path(&thumb),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/social/communities/{id}/raw-thumbnail.png")
            .path(&format!(
                "/social/communities/{}/raw-thumbnail.png",
                MISSING
            ))
            .expect(404),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn write_surface(
    gate: &mut Gate,
    app: &Router,
    user: &Wallet,
    joiner: &Wallet,
    m1: &Wallet,
    m2: &Wallet,
    req_user: &Wallet,
    friend: &Wallet,
) {
    let (mp, mp_ct) = multipart_body(&[
        MultipartPart::field("name", "Gate Community"),
        MultipartPart::field("description", "created by the contract gate"),
    ]);
    gate.hit(
        app,
        Case::new("post", "/v1/communities")
            .signed(user)
            .body(mp.clone(), &mp_ct)
            .expect(201),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities")
            .body(mp.clone(), &mp_ct)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        with_headers(
            Case::new("post", "/v1/communities")
                .body(mp.clone(), &mp_ct)
                .expect(401),
            signed_fetch_headers(user, "post", FORGED),
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities")
            .signed(user)
            .body(b"not-multipart".to_vec(), "text/plain")
            .expect(400),
    )
    .await;

    let c1 = format!("/v1/communities/{}", C1);
    let (upd, upd_ct) = multipart_body(&[MultipartPart::field("name", "Renamed")]);
    gate.hit(
        app,
        Case::new("put", "/v1/communities/{id}")
            .path(&c1)
            .signed(user)
            .body(upd.clone(), &upd_ct),
    )
    .await;
    gate.hit(
        app,
        Case::new("put", "/v1/communities/{id}")
            .path(&c1)
            .body(upd.clone(), &upd_ct)
            .expect(401),
    )
    .await;
    gate.hit(app, forged("put", "/v1/communities/{id}", &c1, user))
        .await;
    gate.hit(
        app,
        Case::new("put", "/v1/communities/{id}")
            .path(&c1)
            .signed(user)
            .body(b"nope".to_vec(), "text/plain")
            .expect(400),
    )
    .await;

    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}")
            .path(&c1)
            .signed(user)
            .json(&json!({ "editorsChoice": true }))
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}")
            .path(&c1)
            .json(&json!({ "editorsChoice": true }))
            .expect(401),
    )
    .await;
    gate.hit(app, forged("patch", "/v1/communities/{id}", &c1, user))
        .await;

    let c_del = format!("/v1/communities/{}", C_DEL);
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}")
            .path(&c_del)
            .signed(user)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}")
            .path(&c1)
            .expect(401),
    )
    .await;
    gate.hit(app, forged("delete", "/v1/communities/{id}", &c1, user))
        .await;

    let members = format!("/v1/communities/{}/members", C1);
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/members")
            .path(&members)
            .signed(joiner)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/members")
            .path(&members)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged("post", "/v1/communities/{id}/members", &members, joiner),
    )
    .await;

    let leave = format!("/v1/communities/{}/members/{}", C1, addr(joiner));
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/members/{address}")
            .path(&leave)
            .signed(joiner)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/members/{address}")
            .path(&leave)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "delete",
            "/v1/communities/{id}/members/{address}",
            &leave,
            joiner,
        ),
    )
    .await;

    let role_path = format!("/v1/communities/{}/members/{}", C1, addr(m1));
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}/members/{address}")
            .path(&role_path)
            .signed(user)
            .json(&json!({ "role": "moderator" }))
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}/members/{address}")
            .path(&role_path)
            .json(&json!({ "role": "moderator" }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "patch",
            "/v1/communities/{id}/members/{address}",
            &role_path,
            user,
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}/members/{address}")
            .path(&role_path)
            .signed(user)
            .json(&json!({ "role": "bogus" }))
            .expect(400),
    )
    .await;

    let ban_path = format!("/v1/communities/{}/members/{}/bans", C1, addr(m2));
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/members/{address}/bans")
            .path(&ban_path)
            .signed(user)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/members/{address}/bans")
            .path(&ban_path)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "post",
            "/v1/communities/{id}/members/{address}/bans",
            &ban_path,
            user,
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/members/{address}/bans")
            .path(&ban_path)
            .signed(user)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/members/{address}/bans")
            .path(&ban_path)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "delete",
            "/v1/communities/{id}/members/{address}/bans",
            &ban_path,
            user,
        ),
    )
    .await;

    let places = format!("/v1/communities/{}/places", C1);
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/places")
            .path(&places)
            .signed(user)
            .json(&json!({ "placeIds": ["place-1"] }))
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/places")
            .path(&places)
            .json(&json!({ "placeIds": ["place-1"] }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged("post", "/v1/communities/{id}/places", &places, user),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/places")
            .path(&places)
            .signed(user)
            .json(&json!({ "placeIds": [] }))
            .expect(400),
    )
    .await;

    let place_del = format!("/v1/communities/{}/places/place-1", C1);
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/places/{placeId}")
            .path(&place_del)
            .signed(user)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/places/{placeId}")
            .path(&place_del)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "delete",
            "/v1/communities/{id}/places/{placeId}",
            &place_del,
            user,
        ),
    )
    .await;

    let posts = format!("/v1/communities/{}/posts", C1);
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/posts")
            .path(&posts)
            .signed(user)
            .json(&json!({ "content": "hello from the gate" }))
            .expect(201),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/posts")
            .path(&posts)
            .json(&json!({ "content": "x" }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged("post", "/v1/communities/{id}/posts", &posts, user),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/posts")
            .path(&posts)
            .signed(user)
            .json(&json!({ "content": "   " }))
            .expect(400),
    )
    .await;

    let post_del = format!("/v1/communities/{}/posts/{}", C1, P1);
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/posts/{postId}")
            .path(&post_del)
            .signed(user)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/posts/{postId}")
            .path(&post_del)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "delete",
            "/v1/communities/{id}/posts/{postId}",
            &post_del,
            user,
        ),
    )
    .await;

    let like = format!("/v1/communities/{}/posts/{}/like", C1, P2);
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/posts/{postId}/like")
            .path(&like)
            .signed(user)
            .expect(201),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/posts/{postId}/like")
            .path(&like)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "post",
            "/v1/communities/{id}/posts/{postId}/like",
            &like,
            user,
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/posts/{postId}/like")
            .path(&like)
            .signed(user)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/communities/{id}/posts/{postId}/like")
            .path(&like)
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "delete",
            "/v1/communities/{id}/posts/{postId}/like",
            &like,
            user,
        ),
    )
    .await;

    let req_priv = format!("/v1/communities/{}/requests", C_PRIV);
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/requests")
            .path(&req_priv)
            .signed(req_user)
            .json(&json!({ "type": "request_to_join" }))
            .expect(200),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/requests")
            .path(&req_priv)
            .json(&json!({ "type": "request_to_join" }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged("post", "/v1/communities/{id}/requests", &req_priv, req_user),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/communities/{id}/requests")
            .path(&req_priv)
            .signed(req_user)
            .json(&json!({ "type": "bogus" }))
            .expect(400),
    )
    .await;

    let req_upd = format!("/v1/communities/{}/requests/{}", C_PRIV, R1);
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}/requests/{requestId}")
            .path(&req_upd)
            .signed(user)
            .json(&json!({ "intention": "accepted" }))
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}/requests/{requestId}")
            .path(&req_upd)
            .json(&json!({ "intention": "accepted" }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        forged(
            "patch",
            "/v1/communities/{id}/requests/{requestId}",
            &req_upd,
            user,
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/communities/{id}/requests/{requestId}")
            .path(&format!("/v1/communities/{}/requests/{}", C_PRIV, R1))
            .signed(user)
            .json(&json!({ "intention": "bogus" }))
            .expect(400),
    )
    .await;

    let mcomms = format!("/v1/members/{}/communities", addr(user));
    gate.hit(
        app,
        Case::new("post", "/v1/members/{address}/communities")
            .path(&mcomms)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "communityIds": [C1] })),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/members/{address}/communities")
            .path(&mcomms)
            .json(&json!({ "communityIds": [C1] }))
            .expect(401),
    )
    .await;

    gate.hit(
        app,
        Case::new("post", "/v1/mutes")
            .signed(user)
            .json(&json!({ "muted_address": addr(m1) }))
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/mutes")
            .json(&json!({ "muted_address": addr(m1) }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        with_headers(
            Case::new("post", "/v1/mutes")
                .json(&json!({ "muted_address": addr(m1) }))
                .expect(401),
            signed_fetch_headers(user, "post", FORGED),
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/mutes")
            .signed(user)
            .json(&json!({ "muted_address": "not-an-address" }))
            .expect(400),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/mutes")
            .signed(user)
            .json(&json!({ "muted_address": addr(m1) }))
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/mutes")
            .json(&json!({ "muted_address": addr(m1) }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        with_headers(
            Case::new("delete", "/v1/mutes")
                .json(&json!({ "muted_address": addr(m1) }))
                .expect(401),
            signed_fetch_headers(user, "delete", FORGED),
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("delete", "/v1/mutes")
            .signed(user)
            .json(&json!({ "muted_address": "not-an-address" }))
            .expect(400),
    )
    .await;

    let send = format!("/v1/friends/{}/messages", addr(friend));
    gate.hit(
        app,
        Case::new("post", "/v1/friends/{peer}/messages")
            .path(&send)
            .signed(user)
            .json(&json!({ "body": "hi" }))
            .expect(200),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/friends/{peer}/messages")
            .path(&send)
            .json(&json!({ "body": "hi" }))
            .expect(401),
    )
    .await;
    gate.hit(
        app,
        with_headers(
            Case::new("post", "/v1/friends/{peer}/messages")
                .path(&send)
                .json(&json!({ "body": "hi" }))
                .expect(401),
            signed_fetch_headers(user, "post", FORGED),
        ),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/friends/{peer}/messages")
            .path(&send)
            .signed(user)
            .json(&json!({ "body": "   " }))
            .expect(400),
    )
    .await;

    // Referral progress: the signer is the invited user for POST/PATCH and the
    // referrer for GET. POST is idempotent for a same-referrer duplicate and
    // first-wins for a different referrer; PATCH walks pending -> signed_up once.
    gate.hit(app, Case::new("get", "/v1/referral-progress").signed(user))
        .await;
    gate.hit(app, Case::new("get", "/v1/referral-progress").expect(401))
        .await;

    let refer_body = json!({ "referrer": addr(user) });
    gate.hit(
        app,
        Case::new("post", "/v1/referral-progress")
            .signed(joiner)
            .json(&refer_body)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/referral-progress")
            .signed(joiner)
            .json(&refer_body)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/referral-progress")
            .signed(joiner)
            .json(&json!({ "referrer": addr(m1) }))
            .expect(400),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/referral-progress")
            .signed(user)
            .json(&refer_body)
            .expect(400),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/referral-progress")
            .json(&refer_body)
            .expect(401),
    )
    .await;

    gate.hit(
        app,
        Case::new("patch", "/v1/referral-progress")
            .signed(joiner)
            .expect(204),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/referral-progress")
            .signed(joiner)
            .expect(400),
    )
    .await;
    gate.hit(
        app,
        Case::new("patch", "/v1/referral-progress")
            .signed(m2)
            .expect(404),
    )
    .await;
    gate.hit(app, Case::new("patch", "/v1/referral-progress").expect(401))
        .await;
}

async fn admin_and_federation(gate: &mut Gate, app: &Router, _user: &Wallet, thumb_hash: &str) {
    gate.hit(
        app,
        Case::new("get", "/v1/admin/communities").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(app, Case::new("get", "/v1/admin/communities").expect(403))
        .await;

    let suspend = format!("/v1/admin/communities/{}/suspend", C1);
    gate.hit(
        app,
        Case::new("post", "/v1/admin/communities/{id}/suspend")
            .path(&suspend)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "reason": "gate" })),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/admin/communities/{id}/suspend")
            .path(&suspend)
            .expect(403),
    )
    .await;

    let unsuspend = format!("/v1/admin/communities/{}/unsuspend", C1);
    gate.hit(
        app,
        Case::new("post", "/v1/admin/communities/{id}/unsuspend")
            .path(&unsuspend)
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/v1/admin/communities/{id}/unsuspend")
            .path(&unsuspend)
            .expect(403),
    )
    .await;

    gate.hit(app, Case::new("get", "/federation/communities/snapshot"))
        .await;
    gate.hit(app, Case::new("get", "/federation/communities/changes"))
        .await;

    gate.hit(
        app,
        Case::new("post", "/federation/communities/content")
            .body(b"gate-content-blob".to_vec(), "application/octet-stream"),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/federation/communities/content")
            .body(vec![0u8; 300 * 1024], "application/octet-stream")
            .expect(413),
    )
    .await;

    gate.hit(
        app,
        Case::new("get", "/federation/communities/content/{hash}")
            .path(&format!("/federation/communities/content/{}", thumb_hash)),
    )
    .await;
    gate.hit(
        app,
        Case::new("get", "/federation/communities/content/{hash}")
            .path(&format!(
                "/federation/communities/content/{}",
                "0".repeat(64)
            ))
            .expect(404),
    )
    .await;

    gate.hit(
        app,
        Case::new("post", "/federation/communities/content/gc").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        app,
        Case::new("post", "/federation/communities/content/gc").expect(401),
    )
    .await;
}
