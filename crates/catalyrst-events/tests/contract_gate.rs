use std::sync::Arc;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_contract_gate::{multipart_body, test_wallet, Case, Gate, MultipartPart};
use catalyrst_events::clients::CommsGatekeeper;
use catalyrst_events::content_store::{ContentStore, MAX_POSTER_BYTES};
use catalyrst_events::ports::attendees::AttendeesComponent;
use catalyrst_events::ports::categories::CategoriesComponent;
use catalyrst_events::ports::events::EventsComponent;
use catalyrst_events::ports::schedules::SchedulesComponent;
use catalyrst_events::{api_router_with_spec, AppState, AppStateInner};
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{Signed, TypedMessage};
use serde_json::{json, Value};
use sqlx::PgPool;

const ADMIN_TOKEN: &str = "contract-gate-admin";

async fn fixture_tables(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE event ( \
           id text PRIMARY KEY, \
           name text NOT NULL, \
           start_at timestamptz, \
           finish_at timestamptz, \
           next_start_at timestamptz, \
           next_finish_at timestamptz, \
           duration_ms bigint, \
           recurrent boolean NOT NULL DEFAULT false, \
           highlighted boolean NOT NULL DEFAULT false, \
           trending boolean NOT NULL DEFAULT false, \
           approved boolean NOT NULL DEFAULT false, \
           attending boolean, \
           community_id text, \
           user_creator text, \
           coordinates_x integer, \
           coordinates_y integer, \
           description text, \
           raw jsonb NOT NULL, \
           fetched_at timestamptz NOT NULL DEFAULT now() )",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE event_attendance_local ( \
           event_id text NOT NULL, \
           signer text NOT NULL, \
           signed_payload jsonb NOT NULL DEFAULT '{}'::jsonb, \
           action text NOT NULL, \
           signed_at timestamptz NOT NULL DEFAULT now(), \
           PRIMARY KEY (event_id, signer) )",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE events_local ( \
           id text PRIMARY KEY, \
           signer text NOT NULL, \
           signed_payload jsonb NOT NULL DEFAULT '{}'::jsonb, \
           signed_at timestamptz NOT NULL DEFAULT now(), \
           updated_at timestamptz NOT NULL DEFAULT now() )",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_event(pool: &PgPool, id: &str, creator: &str) {
    let raw = json!({ "user": creator });
    sqlx::query(
        "INSERT INTO event \
           (id, name, next_start_at, next_finish_at, approved, user_creator, \
            coordinates_x, coordinates_y, raw) \
         VALUES ($1, $2, now() - interval '1 hour', now() + interval '1 hour', \
                 true, $3, 0, 0, $4)",
    )
    .bind(id)
    .bind(format!("event {id}"))
    .bind(creator)
    .bind(raw)
    .execute(pool)
    .await
    .unwrap();
}

async fn add_moderator(pool: &PgPool, address: &str) {
    sqlx::query("INSERT INTO moderators (address, added_at) VALUES ($1, $2)")
        .bind(address.to_ascii_lowercase())
        .bind(chrono::Utc::now().timestamp())
        .execute(pool)
        .await
        .unwrap();
}

fn mk_alloy_wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[0] = 1;
    key[31] = seed;
    PrivateKeySigner::from_slice(&key).unwrap()
}

fn addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

async fn sign_envelope<T: TypedMessage + serde::Serialize>(
    wallet: &PrivateKeySigner,
    message: T,
    domain: Eip712Domain,
) -> Value {
    let mut signed = Signed {
        domain,
        message,
        nonce: rand_nonce(),
        signed_at: chrono::Utc::now().timestamp(),
        signature: String::new(),
    };
    let hash = signed.hash();
    let sig = wallet.sign_message(&hash).await.unwrap();
    signed.signature = sig.to_string();
    serde_json::to_value(&signed).unwrap()
}

fn rand_nonce() -> [u8; 16] {
    use rand::RngExt;
    rand::rng().random()
}

fn build_state(pool: PgPool, content_dir: std::path::PathBuf) -> AppState {
    Arc::new(AppStateInner {
        events: EventsComponent::new(pool.clone(), None),
        attendees: AttendeesComponent::new(pool.clone()),
        categories: CategoriesComponent::new(pool.clone()),
        schedules: SchedulesComponent::new(pool.clone()),
        admin_token: Some(ADMIN_TOKEN.into()),
        pool,
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: domains::events(),
        content_store: Arc::new(ContentStore::new(content_dir, MAX_POSTER_BYTES)),
        comms: CommsGatekeeper::new("http://127.0.0.1:9".into()),
    })
}

fn png_part() -> MultipartPart {
    MultipartPart::file(
        "poster",
        "poster.png",
        "image/png",
        vec![0x89, 0x50, 0x4e, 0x47, 1, 2, 3],
    )
}

#[tokio::test]
async fn every_spec_route_answers_its_contract() {
    let Some(scratch) = ScratchSchema::create("CATALYRST_EVENTS_TEST_PG", "cg_events").await else {
        return;
    };
    scratch
        .apply_sql(include_str!("../migrations/0001_federation.sql"))
        .await;
    fixture_tables(&scratch.pool).await;

    let user = test_wallet(7);
    let moderator = mk_alloy_wallet(11);
    let moderator_fetch = test_wallet(11);
    assert_eq!(addr(&moderator), moderator_fetch.address());
    add_moderator(&scratch.pool, &addr(&moderator)).await;
    seed_event(&scratch.pool, "ev-1", &user.address().to_lowercase()).await;
    seed_event(&scratch.pool, "ev-del", &user.address().to_lowercase()).await;

    let content_dir = std::env::temp_dir().join(format!("cg-events-{}", scratch.schema));
    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone(), content_dir.clone());
    state.content_store.init().await.unwrap();
    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    gate.hit(&app, Case::new("get", "/api/events")).await;
    gate.hit(
        &app,
        Case::new("get", "/api/events")
            .query("owner=true")
            .expect(401),
    )
    .await;

    gate.hit(
        &app,
        Case::new("post", "/api/events")
            .bearer(ADMIN_TOKEN)
            .json(&json!({
                "name": "Gate Party",
                "start_at": "2030-01-01T10:00:00Z",
                "duration": 3_600_000,
                "x": 0,
                "y": 0
            })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/events")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "name": "  " }))
            .expect(400),
    )
    .await;

    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/events")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "name": "Sanitize Party",
                    "start_at": "2030-01-01T10:00:00Z",
                    "duration": 3_600_000,
                    "x": 0,
                    "y": 0,
                    "description": "x <link=\"file:///etc/passwd\">y</link>"
                })),
        )
        .await;
    assert_eq!(
        created["data"]["description"],
        json!("x y"),
        "create_event must echo a sanitized description"
    );

    gate.hit(
        &app,
        Case::new("post", "/api/events/search").json(&json!({})),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/events/search")
            .query("owner=true")
            .json(&json!({}))
            .expect(401),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/events/attending").signed(&user),
    )
    .await;
    gate.hit(&app, Case::new("get", "/api/events/attending").expect(401))
        .await;

    gate.hit(&app, Case::new("get", "/api/events/categories"))
        .await;

    gate.hit(
        &app,
        Case::new("get", "/api/events/moderation").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(&app, Case::new("get", "/api/events/moderation").expect(403))
        .await;

    gate.hit(
        &app,
        Case::new("get", "/api/events/{event_id}").path("/api/events/ev-1"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/api/events/{event_id}")
            .path("/api/events/nope")
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("patch", "/api/events/{event_id}")
            .path("/api/events/ev-1")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "highlighted": true })),
    )
    .await;
    let patched = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-1")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "description": "x <link=\"file:///etc/passwd\">y</link>"
                })),
        )
        .await;
    assert_eq!(
        patched["data"]["description"],
        json!("x y"),
        "patch_event must echo a sanitized description"
    );
    gate.hit(
        &app,
        Case::new("patch", "/api/events/{event_id}")
            .path("/api/events/nope")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "highlighted": true }))
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("delete", "/api/events/{event_id}")
            .path("/api/events/ev-1")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/events/{event_id}")
            .path("/api/events/ev-del")
            .bearer(ADMIN_TOKEN)
            .expect(200),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/events/{event_id}/attendees").path("/api/events/ev-1/attendees"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/events/{event_id}/attendees")
            .path("/api/events/ev-1/attendees")
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/events/{event_id}/attendees")
            .path("/api/events/nope/attendees")
            .signed(&user)
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/events/{event_id}/attendees")
            .path("/api/events/ev-1/attendees")
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/events/{event_id}/attendees")
            .path("/api/events/ev-1/attendees")
            .expect(401),
    )
    .await;

    let (poster, poster_type) = multipart_body(&[png_part()]);
    let uploaded_poster = gate
        .hit(
            &app,
            Case::new("post", "/api/poster")
                .signed(&user)
                .body(poster, &poster_type),
        )
        .await;
    let (poster, poster_type) = multipart_body(&[png_part()]);
    gate.hit(
        &app,
        Case::new("post", "/api/poster")
            .body(poster, &poster_type)
            .expect(401),
    )
    .await;

    let (poster, poster_type) = multipart_body(&[png_part()]);
    let uploaded_vertical = gate
        .hit(
            &app,
            Case::new("post", "/api/poster-vertical")
                .signed(&user)
                .body(poster, &poster_type),
        )
        .await;
    let (poster, poster_type) = multipart_body(&[png_part()]);
    gate.hit(
        &app,
        Case::new("post", "/api/poster-vertical")
            .body(poster, &poster_type)
            .expect(401),
    )
    .await;

    let poster_url = uploaded_poster["url"].as_str().expect("poster url");
    gate.hit(
        &app,
        Case::new("get", "/poster/{filename}").path(poster_url),
    )
    .await;
    let vertical_url = uploaded_vertical["url"].as_str().expect("poster url");
    gate.hit(
        &app,
        Case::new("get", "/poster-vertical/{filename}").path(vertical_url),
    )
    .await;

    let absent = "0000000000000000000000000000000000000000000000000000000000000000.png";
    gate.hit(
        &app,
        Case::new("get", "/poster/{filename}")
            .path(&format!("/poster/{absent}"))
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/poster-vertical/{filename}")
            .path(&format!("/poster-vertical/{absent}"))
            .expect(404),
    )
    .await;

    let create_schedule = sign_envelope(
        &moderator,
        catalyrst_events::fed::messages::ScheduleUpsert {
            schedule_id: None,
            name: "Gate Fest".into(),
            description: None,
            image: None,
            theme: None,
            background: vec!["#fff".into()],
            active_since: chrono::Utc::now().timestamp(),
            active_until: chrono::Utc::now().timestamp() + 3600,
            active: true,
            signed_at: chrono::Utc::now().timestamp(),
        },
        domains::events(),
    )
    .await;
    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/schedules").json(&create_schedule),
        )
        .await;
    let schedule_id = created["data"]["id"].as_str().unwrap().to_string();
    gate.hit(
        &app,
        Case::new("post", "/api/schedules")
            .json(&json!({ "name": "no envelope" }))
            .expect(400),
    )
    .await;

    gate.hit(&app, Case::new("get", "/api/schedules")).await;

    gate.hit(
        &app,
        Case::new("get", "/api/schedules/{schedule_id}")
            .path(&format!("/api/schedules/{}", schedule_id)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/api/schedules/{schedule_id}")
            .path("/api/schedules/nope")
            .expect(404),
    )
    .await;

    let patch_schedule = sign_envelope(
        &moderator,
        catalyrst_events::fed::messages::ScheduleUpsert {
            schedule_id: Some(schedule_id.clone()),
            name: "Gate Fest 2".into(),
            description: None,
            image: None,
            theme: None,
            background: vec![],
            active_since: chrono::Utc::now().timestamp(),
            active_until: chrono::Utc::now().timestamp() + 60,
            active: false,
            signed_at: chrono::Utc::now().timestamp(),
        },
        domains::events(),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/schedules/{schedule_id}")
            .path(&format!("/api/schedules/{}", schedule_id))
            .json(&patch_schedule),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/schedules/{schedule_id}")
            .path(&format!("/api/schedules/{}", schedule_id))
            .json(&json!({ "name": "no envelope" }))
            .expect(400),
    )
    .await;

    let my_settings = sign_envelope(
        &moderator,
        catalyrst_events::fed::messages::ProfileSettingsUpdate {
            target: addr(&moderator),
            email: Some("gate@example.com".into()),
            email_verified: None,
            use_local_time: None,
            notify_by_email: None,
            notify_by_browser: None,
            permissions: None,
            signed_at: chrono::Utc::now().timestamp(),
        },
        domains::events(),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/profiles/me/settings").json(&my_settings),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/profiles/me/settings")
            .json(&json!({ "email": "gate@example.com" }))
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/profiles/me/settings").signed(&moderator_fetch),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/api/profiles/me/settings").expect(401),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/profiles/settings").signed(&moderator_fetch),
    )
    .await;
    gate.hit(&app, Case::new("get", "/api/profiles/settings").expect(401))
        .await;

    let profile_path = format!("/api/profiles/{}/settings", addr(&moderator));
    gate.hit(
        &app,
        Case::new("get", "/api/profiles/{profile_id}/settings")
            .path(&profile_path)
            .signed(&moderator_fetch),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/api/profiles/{profile_id}/settings")
            .path(&profile_path)
            .expect(401),
    )
    .await;

    let admin_settings = sign_envelope(
        &moderator,
        catalyrst_events::fed::messages::ProfileSettingsUpdate {
            target: addr(&moderator),
            email: None,
            email_verified: Some(true),
            use_local_time: None,
            notify_by_email: None,
            notify_by_browser: None,
            permissions: None,
            signed_at: chrono::Utc::now().timestamp(),
        },
        domains::events(),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/profiles/{profile_id}/settings")
            .path(&profile_path)
            .json(&admin_settings),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/profiles/{profile_id}/settings")
            .path(&profile_path)
            .json(&json!({ "email": "gate@example.com" }))
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/profiles/subscriptions").expect(410),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/profiles/subscriptions").expect(410),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/profiles/subscriptions").expect(410),
    )
    .await;

    gate.hit(&app, Case::new("get", "/events/sitemap.xml"))
        .await;
    gate.hit(&app, Case::new("get", "/events/sitemap.static.xml"))
        .await;
    gate.hit(&app, Case::new("get", "/events/sitemap.events.xml"))
        .await;
    gate.hit(&app, Case::new("get", "/events/sitemap.schedules.xml"))
        .await;

    gate.hit(&app, Case::new("get", "/federation/v1/events/feed"))
        .await;
    gate.hit(
        &app,
        Case::new("get", "/federation/v1/events/{event_id}/attendance")
            .path("/federation/v1/events/ev-1/attendance"),
    )
    .await;

    gate.assert_covered();

    let _ = std::fs::remove_dir_all(&content_dir);
    scratch.drop().await;
}
