use std::sync::Arc;

use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::{BanWriteError, CreateBan, UserBansComponent};
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

async fn setup_db() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_banhard").await?;
    apply_migration(&scratch.pool, include_str!("../migrations/0001_comms.sql")).await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0002_user_moderation.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0003_private_messages_privacy.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0004_mls_messaging.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0005_published_events.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0006_player_connection_and_device_bans.sql"),
    )
    .await;
    apply_migration(
        &scratch.pool,
        include_str!("../migrations/0007_community_voice_chat_sid.sql"),
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

fn test_state(pool: PgPool) -> AppState {
    state_with_places(pool, None)
}

fn state_with_places(pool: PgPool, places_pool: Option<PgPool>) -> AppState {
    state_with_content_urls(
        pool,
        places_pool,
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
    )
}

fn state_with_content_urls(
    pool: PgPool,
    places_pool: Option<PgPool>,
    world_content_url: &str,
    lambdas_url: &str,
) -> AppState {
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        player_reports: PlayerReportsComponent::new(pool.clone()),
        names: NamesComponent::new(None, "squid_marketplace".into()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool,
        dapps_pool: None,
        dapps_schema: "squid_marketplace".into(),
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: world_content_url.into(),
        lambdas_url: lambdas_url.into(),
        pool,
        livekit_api_url: String::new(),
        livekit_ws_url: String::new(),
        livekit_api_key: String::new(),
        livekit_api_secret: String::new(),
        livekit_webhook_key: None,
        livekit_configured: false,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
        fed_peer_id: "test-peer".into(),
        world_permissions: Default::default(),
    })
}

#[tokio::test]
async fn concurrent_create_ban_yields_single_active_ban() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let victim = "0x1111111111111111111111111111111111111111";
    let moderator = "0x9999999999999999999999999999999999999999";

    let mut handles = Vec::new();
    for _ in 0..8 {
        let bans = UserBansComponent::new(pool.clone());
        handles.push(tokio::spawn(async move {
            bans.create_ban(CreateBan {
                banned_address: victim.into(),
                banned_by: moderator.into(),
                reason: "abuse".into(),
                custom_message: None,
                banned_device_id: None,
                duration_ms: None,
            })
            .await
        }));
    }

    let mut created = 0;
    let mut already_banned = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(_) => created += 1,
            Err(BanWriteError::AlreadyBanned(addr)) => {
                assert_eq!(addr, victim);
                already_banned += 1;
            }
            Err(BanWriteError::Db(e)) => panic!("unexpected db error: {e}"),
        }
    }
    assert_eq!(created, 1, "exactly one concurrent ban must win");
    assert_eq!(already_banned, 7);

    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_bans \
         WHERE banned_address = $1 AND lifted_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(victim)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        active, 1,
        "the advisory lock must prevent duplicate active bans"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn scene_ban_rejects_admin_target() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let state = test_state(pool.clone());
    let place_id = "place-1";
    let admin = "0xAAAA111111111111111111111111111111111111";
    let owner = "0x9999999999999999999999999999999999999999";
    let bystander = "0x2222222222222222222222222222222222222222";

    state.scene_admin.add(place_id, admin, owner).await.unwrap();

    let err =
        catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(&state, place_id, admin)
            .await
            .expect_err("banning a scene admin must be rejected");
    assert_eq!(err.code, 400);
    assert_eq!(err.message, "Cannot ban this address");

    catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(&state, place_id, bystander)
        .await
        .expect("a regular user must remain bannable");

    scratch.drop().await;
}

/// The protection list behind `ensure_target_not_protected` is the only thing
/// standing between an admin and a ban on an owner, an extra address or a lease
/// holder. A places lookup that cannot run reads that list empty, so it must be
/// answered the way `resolve_rooms` answers it -- 503, not "nobody is protected".
#[tokio::test]
async fn a_places_lookup_that_cannot_run_does_not_clear_a_ban_target() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    let faulted_places = PgPool::connect_lazy("postgres://postgres@127.0.0.1:1/postgres")
        .expect("lazy pool never connects in this test");
    faulted_places.close().await;
    let state = state_with_places(scratch.pool.clone(), Some(faulted_places));

    let err = catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(
        &state,
        "place-1",
        "0x2222222222222222222222222222222222222222",
    )
    .await
    .expect_err("a places pool that cannot answer must not read as an unprotected target");
    assert_eq!(err.code, 503);
    assert_eq!(
        err.message,
        "the place catalog is unavailable, so this request cannot be bound to its room"
    );

    scratch.drop().await;
}

/// The same hazard one layer down: for a WORLD place the protection list is
/// assembled from the worlds content server, and a fetch that cannot run used to
/// read as "nobody is protected", so a world owner or an allow-listed deployer
/// became bannable while the service was down. Upstream lets the lookup throw
/// (`getUserScenePermissions` in adapters/scene-manager.ts), never reads the
/// permission flags as false.
#[tokio::test]
async fn a_world_permission_fetch_that_cannot_run_does_not_clear_a_ban_target() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    sqlx::query(
        "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, \
         base_position text NOT NULL DEFAULT '0,0');",
    )
    .execute(&scratch.pool)
    .await
    .expect("create place table");
    sqlx::query("INSERT INTO place (id, raw, base_position) VALUES ($1, $2::jsonb, $3)")
        .bind("place-world")
        .bind(r#"{"world": true, "world_name": "foo.dcl.eth", "positions": ["0,0"]}"#)
        .bind("0,0")
        .execute(&scratch.pool)
        .await
        .expect("insert world place");

    let state = state_with_places(scratch.pool.clone(), Some(scratch.pool.clone()));

    let err = catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(
        &state,
        "place-world",
        "0x2222222222222222222222222222222222222222",
    )
    .await
    .expect_err("a world permission surface that answers nothing must not clear the target");
    assert_eq!(err.code, 503);
    assert_eq!(
        err.message,
        catalyrst_comms::ports::extra_addresses::WORLD_PERMISSIONS_UNAVAILABLE_MSG
    );

    scratch.drop().await;
}

/// The genesis twin of the world case above: the protection set for a non-world
/// place is the LAND owner, operator, updateOperator, updateManagers and
/// approvedForAll, read from lambdas. Upstream's `getLandOperators`
/// (adapters/lands/component.ts) throws when that read cannot run, so a lambdas
/// outage must not hand an admin a green light to ban a land operator.
#[tokio::test]
async fn a_land_operator_fetch_that_cannot_run_does_not_clear_a_ban_target() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    sqlx::query(
        "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, \
         base_position text NOT NULL DEFAULT '0,0');",
    )
    .execute(&scratch.pool)
    .await
    .expect("create place table");
    sqlx::query("INSERT INTO place (id, raw, base_position) VALUES ($1, $2::jsonb, $3)")
        .bind("place-genesis")
        .bind(r#"{"world": false, "positions": ["10,20"]}"#)
        .bind("10,20")
        .execute(&scratch.pool)
        .await
        .expect("insert genesis place");

    let state = state_with_places(scratch.pool.clone(), Some(scratch.pool.clone()));

    let err = catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(
        &state,
        "place-genesis",
        "0x2222222222222222222222222222222222222222",
    )
    .await
    .expect_err("a land operator read that cannot run must not clear the target");
    assert_eq!(err.code, 503);
    assert_eq!(
        err.message,
        catalyrst_comms::ports::extra_addresses::LAND_OPERATORS_UNAVAILABLE_MSG
    );

    scratch.drop().await;
}

async fn world_permissions_stub() -> std::net::SocketAddr {
    use axum::response::IntoResponse;

    let app = axum::Router::new().fallback(|req: axum::extract::Request| async move {
        if req.method() == axum::http::Method::POST && req.uri().path().ends_with("/parcels") {
            return (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"total":0,"addresses":[]}"#,
            )
                .into_response();
        }
        axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// A half-degraded worlds content server -- the bulk parcel endpoints answering
/// while the permission document alone 5xx's -- used to take the fast path with
/// no owner in the set, so the world owner recorded by the worlds server became
/// bannable. Upstream puts `fetchWorldActionPermissions` inside the same
/// Promise.all (adapters/scene-admins.ts) and re-awaits it outside a try on the
/// fallback, so either way the read rejects.
#[tokio::test]
async fn a_world_permission_document_that_5xxs_does_not_clear_a_ban_target() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    sqlx::query(
        "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, \
         base_position text NOT NULL DEFAULT '0,0');",
    )
    .execute(&scratch.pool)
    .await
    .expect("create place table");
    sqlx::query("INSERT INTO place (id, raw, base_position) VALUES ($1, $2::jsonb, $3)")
        .bind("place-world")
        .bind(r#"{"world": true, "world_name": "foo.dcl.eth", "positions": ["0,0"]}"#)
        .bind("0,0")
        .execute(&scratch.pool)
        .await
        .expect("insert world place");

    let addr = world_permissions_stub().await;
    let state = state_with_content_urls(
        scratch.pool.clone(),
        Some(scratch.pool.clone()),
        &format!("http://{addr}"),
        "http://127.0.0.1:1",
    );

    let err = catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(
        &state,
        "place-world",
        "0x2222222222222222222222222222222222222222",
    )
    .await
    .expect_err("a permission document that cannot be read must not clear the target");
    assert_eq!(err.code, 503);
    assert_eq!(
        err.message,
        catalyrst_comms::ports::extra_addresses::WORLD_PERMISSIONS_UNAVAILABLE_MSG
    );

    scratch.drop().await;
}

/// The same hole with no stub at all: a world place that lists no parcel makes
/// both bulk reads short-circuit to Ok(empty), so the fast path was taken even
/// with the whole worlds server down.
#[tokio::test]
async fn a_world_place_with_no_parcels_still_fails_closed_on_a_permission_outage() {
    let Some(scratch) = setup_db().await else {
        return;
    };
    sqlx::query(
        "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, base_position text);",
    )
    .execute(&scratch.pool)
    .await
    .expect("create place table");
    sqlx::query("INSERT INTO place (id, raw, base_position) VALUES ($1, $2::jsonb, NULL)")
        .bind("place-world-empty")
        .bind(r#"{"world": true, "world_name": "bar.dcl.eth", "positions": []}"#)
        .execute(&scratch.pool)
        .await
        .expect("insert world place with no parcels");

    let state = state_with_places(scratch.pool.clone(), Some(scratch.pool.clone()));

    let err = catalyrst_comms::handlers::scene_bans::ensure_target_not_protected(
        &state,
        "place-world-empty",
        "0x2222222222222222222222222222222222222222",
    )
    .await
    .expect_err("an empty parcel list must not turn an outage into an empty protection set");
    assert_eq!(err.code, 503);
    assert_eq!(
        err.message,
        catalyrst_comms::ports::extra_addresses::WORLD_PERMISSIONS_UNAVAILABLE_MSG
    );

    scratch.drop().await;
}
