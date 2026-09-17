use std::sync::Arc;

use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::Router;
use catalyrst_contract_gate::MultipartPart;
use catalyrst_events::clients::{CommsGatekeeper, Places};
use catalyrst_events::content_store::{ContentStore, MAX_POSTER_BYTES};
use catalyrst_events::ports::attendees::AttendeesComponent;
use catalyrst_events::ports::categories::CategoriesComponent;
use catalyrst_events::ports::events::EventsComponent;
use catalyrst_events::ports::schedules::SchedulesComponent;
use catalyrst_events::{AppState, AppStateInner};
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{Signed, TypedMessage};
use serde_json::{json, Value};
use sqlx::PgPool;

pub const ADMIN_TOKEN: &str = "contract-gate-admin";

pub async fn fixture_tables(pool: &PgPool) {
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

pub async fn seed_event(pool: &PgPool, id: &str, creator: &str) {
    seed_event_with(pool, id, creator, true).await;
}

pub async fn seed_event_with(pool: &PgPool, id: &str, creator: &str, approved: bool) {
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

pub const PLACE_UUID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

pub async fn stub_places() -> String {
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

pub fn listed_ids(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["id"].as_str().map(String::from))
        .collect()
}

pub async fn add_moderator(pool: &PgPool, address: &str) {
    sqlx::query("INSERT INTO moderators (address, added_at) VALUES ($1, $2)")
        .bind(address.to_ascii_lowercase())
        .bind(chrono::Utc::now().timestamp())
        .execute(pool)
        .await
        .unwrap();
}

pub fn mk_alloy_wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[0] = 1;
    key[31] = seed;
    PrivateKeySigner::from_slice(&key).unwrap()
}

pub fn addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

pub async fn sign_envelope<T: TypedMessage + serde::Serialize>(
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

pub fn build_state(pool: PgPool, content_dir: std::path::PathBuf, places_url: String) -> AppState {
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

pub const UUID_EVENT_ID: &str = "11111111-2222-4333-8444-555555555555";

pub fn png_part() -> MultipartPart {
    MultipartPart::file(
        "poster",
        "poster.png",
        "image/png",
        vec![0x89, 0x50, 0x4e, 0x47, 1, 2, 3],
    )
}
