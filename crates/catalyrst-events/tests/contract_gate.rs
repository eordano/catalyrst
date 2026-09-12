use std::sync::Arc;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_contract_gate::{multipart_body, test_wallet, Case, Gate, MultipartPart};
use catalyrst_events::clients::{CommsGatekeeper, Places};
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
    seed_event_with(pool, id, creator, true).await;
}

async fn seed_event_with(pool: &PgPool, id: &str, creator: &str, approved: bool) {
    let raw = json!({ "user": creator, "approved": approved });
    sqlx::query(
        "INSERT INTO event \
           (id, name, next_start_at, next_finish_at, approved, user_creator, \
            coordinates_x, coordinates_y, raw) \
         VALUES ($1, $2, now() - interval '1 hour', now() + interval '1 hour', \
                 $5, $3, 0, 0, $4)",
    )
    .bind(id)
    .bind(format!("event {id}"))
    .bind(creator)
    .bind(raw)
    .bind(approved)
    .execute(pool)
    .await
    .unwrap();
}

const PLACE_UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

async fn stub_places() -> String {
    use axum::extract::Query;
    use axum::routing::get;
    let app = Router::new()
        .route(
            "/api/places",
            get(|Query(q): Query<Vec<(String, String)>>| async move {
                let hit = q.iter().any(|(k, v)| k == "positions" && v == "1,2");
                let data = if hit {
                    json!([{ "id": PLACE_UUID, "world": false }])
                } else {
                    json!([])
                };
                axum::Json(json!({ "ok": true, "data": data, "total": 0 }))
            }),
        )
        .route(
            "/api/worlds",
            get(|Query(q): Query<Vec<(String, String)>>| async move {
                let hit = q
                    .iter()
                    .any(|(k, v)| k == "names" && v.eq_ignore_ascii_case("my-world.dcl.eth"));
                let data = if hit {
                    json!([{ "id": "world-row-uuid", "world_name": "My-World.dcl.eth" }])
                } else {
                    json!([])
                };
                axum::Json(json!({ "ok": true, "data": data, "total": 0 }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn listed_ids(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["id"].as_str().map(String::from))
        .collect()
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

fn build_state(pool: PgPool, content_dir: std::path::PathBuf, places_url: String) -> AppState {
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
        places: Places::new(places_url),
    })
}

const UUID_EVENT_ID: &str = "11111111-2222-4333-8444-555555555555";

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
    seed_event(&scratch.pool, "ev-move", &user.address().to_lowercase()).await;
    seed_event_with(
        &scratch.pool,
        "ev-pending",
        &user.address().to_lowercase(),
        false,
    )
    .await;
    seed_event(&scratch.pool, UUID_EVENT_ID, &user.address().to_lowercase()).await;

    let content_dir = std::env::temp_dir().join(format!("cg-events-{}", scratch.schema));
    let (router, spec) = api_router_with_spec();
    let state = build_state(
        scratch.pool.clone(),
        content_dir.clone(),
        stub_places().await,
    );
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

    let anonymous = gate
        .hit(&app, Case::new("get", "/api/events").query("list=all"))
        .await;
    assert!(
        !listed_ids(&anonymous).contains(&"ev-pending".to_string()),
        "a pending event is not public: {anonymous}"
    );
    let own = gate
        .hit(
            &app,
            Case::new("get", "/api/events")
                .query("list=all")
                .signed(&user),
        )
        .await;
    assert!(
        listed_ids(&own).contains(&"ev-pending".to_string()),
        "a signed viewer sees their own pending event: {own}"
    );
    let moderated = gate
        .hit(
            &app,
            Case::new("get", "/api/events")
                .query("list=all")
                .signed(&moderator_fetch),
        )
        .await;
    assert!(
        listed_ids(&moderated).contains(&"ev-pending".to_string()),
        "a signed moderator sees pending events: {moderated}"
    );
    let bearer = gate
        .hit(
            &app,
            Case::new("get", "/api/events")
                .query("list=all")
                .bearer(ADMIN_TOKEN),
        )
        .await;
    assert!(
        listed_ids(&bearer).contains(&"ev-pending".to_string()),
        "the admin bearer sees pending events: {bearer}"
    );

    let created = gate
        .hit(
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
    assert_eq!(
        created["data"]["place_id"],
        Value::Null,
        "a parcel no place covers leaves place_id null"
    );
    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/events")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "name": "Placed Party",
                    "start_at": "2030-01-01T10:00:00Z",
                    "duration": 3_600_000,
                    "x": 1,
                    "y": 2
                })),
        )
        .await;
    assert_eq!(
        created["data"]["place_id"],
        json!(PLACE_UUID),
        "create_event resolves place_id from the parcel"
    );
    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/events")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "name": "World Party",
                    "start_at": "2030-01-01T10:00:00Z",
                    "duration": 3_600_000,
                    "x": 0,
                    "y": 0,
                    "world": true,
                    "server": "My-World.dcl.eth"
                })),
        )
        .await;
    assert_eq!(
        created["data"]["place_id"],
        json!("my-world.dcl.eth"),
        "create_event resolves a world event to its world id"
    );
    assert_eq!(created["data"]["world"], json!(true));
    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/events")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "name": "Flagged Party",
                    "start_at": "2030-01-01T10:00:00Z",
                    "duration": 3_600_000,
                    "x": 1,
                    "y": 2,
                    "world": true
                })),
        )
        .await;
    assert_eq!(
        (&created["data"]["world"], &created["data"]["place_id"]),
        (&json!(false), &json!(PLACE_UUID)),
        "a world flag without a server stores a genesis event at its parcel"
    );
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

    let featured_item =
        "urn:decentraland:matic:collections-v2:0x1234567890abcdef1234567890abcdef12345678:1";
    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/events")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "name": "Featured Party",
                    "start_at": "2030-01-01T10:00:00Z",
                    "duration": 3_600_000,
                    "x": 0,
                    "y": 0,
                    "featured_item": featured_item
                })),
        )
        .await;
    assert_eq!(
        created["data"]["featured_item"],
        json!(featured_item),
        "create_event must echo featured_item"
    );
    let created = gate
        .hit(
            &app,
            Case::new("post", "/api/events")
                .bearer(ADMIN_TOKEN)
                .json(&json!({
                    "name": "Unfeatured Party",
                    "start_at": "2030-01-01T10:00:00Z",
                    "duration": 3_600_000,
                    "x": 0,
                    "y": 0,
                    "featured_item": ""
                })),
        )
        .await;
    assert_eq!(
        created["data"]["featured_item"],
        Value::Null,
        "create_event must normalize an empty featured_item to null"
    );
    gate.hit(
        &app,
        Case::new("post", "/api/events")
            .bearer(ADMIN_TOKEN)
            .json(&json!({
                "name": "Bad Featured Party",
                "start_at": "2030-01-01T10:00:00Z",
                "duration": 3_600_000,
                "x": 0,
                "y": 0,
                "featured_item": "urn:decentraland:matic:collections-v1:0x1234567890abcdef1234567890abcdef12345678:1"
            }))
            .expect(400),
    )
    .await;

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
    let anonymous = gate
        .hit(
            &app,
            Case::new("post", "/api/events/search")
                .query("list=all")
                .json(&json!({})),
        )
        .await;
    assert!(
        !listed_ids(&anonymous).contains(&"ev-pending".to_string()),
        "search hides a pending event from the public: {anonymous}"
    );
    let moderated = gate
        .hit(
            &app,
            Case::new("post", "/api/events/search")
                .query("list=all")
                .signed(&moderator_fetch)
                .json(&json!({})),
        )
        .await;
    assert!(
        listed_ids(&moderated).contains(&"ev-pending".to_string()),
        "search shows a signed moderator pending events: {moderated}"
    );
    let own = gate
        .hit(
            &app,
            Case::new("post", "/api/events/search")
                .query("list=all")
                .signed(&user)
                .json(&json!({})),
        )
        .await;
    assert!(
        listed_ids(&own).contains(&"ev-pending".to_string()),
        "search shows a signed viewer their own pending event: {own}"
    );

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

    gate.hit(&app, Case::new("get", "/v1/events")).await;
    gate.hit(
        &app,
        Case::new("get", "/v1/events")
            .query("owner=true")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/events/{event_id}").path(&format!("/v1/events/{}", UUID_EVENT_ID)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/events/{event_id}")
            .path(&format!("/v1/events/{}", UUID_EVENT_ID))
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/events/{event_id}")
            .path("/v1/events/ev-1")
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}/events")
            .path("/v1/destinations/my-world.dcl.eth/events"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}/events")
            .path("/v1/destinations/my-world.dcl.eth/events")
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}/events")
            .path("/v1/destinations/my-world.dcl.eth/events")
            .header("x-identity-auth-chain-0", "{}")
            .header(
                "x-identity-metadata",
                r#"{"signer":"Decentraland-Kernel-Scene"}"#,
            )
            .expect(400),
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

    let patched = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-1")
                .signed(&user)
                .json(&json!({ "featured_item": "" })),
        )
        .await;
    assert_eq!(patched["data"]["featured_item"], Value::Null);
    assert_eq!(
        patched["data"]["approved"],
        json!(true),
        "clearing an already-empty featured_item must not re-queue moderation"
    );
    let patched = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-1")
                .signed(&user)
                .json(&json!({ "featured_item": featured_item })),
        )
        .await;
    assert_eq!(patched["data"]["featured_item"], json!(featured_item));
    assert_eq!(
        patched["data"]["approved"],
        json!(false),
        "an owner setting featured_item must re-queue the approved event"
    );
    gate.hit(
        &app,
        Case::new("patch", "/api/events/{event_id}")
            .path("/api/events/ev-1")
            .signed(&user)
            .json(&json!({ "featured_item": "my favourite wearable" }))
            .expect(400),
    )
    .await;
    let patched = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-1")
                .signed(&user)
                .json(&json!({ "featured_item": null })),
        )
        .await;
    assert_eq!(patched["data"]["featured_item"], Value::Null);

    let moved = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-move")
                .signed(&user)
                .json(&json!({ "x": 1, "y": 2 })),
        )
        .await;
    assert_eq!(
        moved["data"]["place_id"],
        json!(PLACE_UUID),
        "a position edit re-resolves place_id"
    );
    let moved = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-move")
                .signed(&user)
                .json(&json!({ "world": true, "server": "My-World.dcl.eth" })),
        )
        .await;
    assert_eq!(
        moved["data"]["place_id"],
        json!("my-world.dcl.eth"),
        "a world edit re-resolves place_id to the world id"
    );
    assert_eq!(moved["data"]["world"], json!(true));
    let renamed = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-move")
                .signed(&user)
                .json(&json!({ "name": "Moved Party" })),
        )
        .await;
    assert_eq!(
        renamed["data"]["place_id"],
        json!("my-world.dcl.eth"),
        "an edit that leaves the location alone keeps place_id"
    );
    let cleared = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-move")
                .signed(&user)
                .json(&json!({ "server": null })),
        )
        .await;
    assert_eq!(
        (
            &cleared["data"]["server"],
            &cleared["data"]["world"],
            &cleared["data"]["place_id"]
        ),
        (&Value::Null, &json!(false), &json!(PLACE_UUID)),
        "an explicit null server moves the event back to its parcel"
    );
    let moved = gate
        .hit(
            &app,
            Case::new("patch", "/api/events/{event_id}")
                .path("/api/events/ev-move")
                .signed(&user)
                .json(&json!({ "world": false, "x": 0 })),
        )
        .await;
    assert_eq!(
        moved["data"]["place_id"],
        Value::Null,
        "moving to a parcel no place covers clears place_id"
    );

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
    let hidden = gate
        .hit(
            &app,
            Case::new("get", "/api/events")
                .query("list=all")
                .bearer(ADMIN_TOKEN),
        )
        .await;
    assert!(
        !listed_ids(&hidden).contains(&"ev-del".to_string()),
        "a deleted event stays hidden without allow_deleted: {hidden}"
    );
    let shown = gate
        .hit(
            &app,
            Case::new("get", "/api/events")
                .query("list=all&allow_deleted=true")
                .bearer(ADMIN_TOKEN),
        )
        .await;
    assert!(
        listed_ids(&shown).contains(&"ev-del".to_string()),
        "an admin opting in with allow_deleted sees the deleted event: {shown}"
    );
    let public = gate
        .hit(
            &app,
            Case::new("get", "/api/events").query("list=all&allow_deleted=true"),
        )
        .await;
    assert!(
        !listed_ids(&public).contains(&"ev-del".to_string()),
        "allow_deleted is admin-only: {public}"
    );
    for query in ["list=all&allow_deleted=true", "list=all&deleted=true"] {
        let moderated = gate
            .hit(
                &app,
                Case::new("get", "/api/events")
                    .query(query)
                    .signed(&moderator_fetch),
            )
            .await;
        let ids = listed_ids(&moderated);
        assert!(
            !ids.contains(&"ev-del".to_string()) && ids.contains(&"ev-pending".to_string()),
            "a moderator sees pending events but deleted rows stay behind the bearer ({query}): {moderated}"
        );
    }
    let bearer = gate
        .hit(
            &app,
            Case::new("get", "/api/events")
                .query("list=all&deleted=true")
                .bearer(ADMIN_TOKEN),
        )
        .await;
    let ids = listed_ids(&bearer);
    assert!(
        ids.contains(&"ev-del".to_string()) && !ids.contains(&"ev-1".to_string()),
        "the admin bearer selects only deleted rows: {bearer}"
    );

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
