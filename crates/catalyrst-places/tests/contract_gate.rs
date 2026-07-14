use std::sync::Arc;

use axum::extract::Path;
use axum::routing::{get, post};
use axum::{Json, Router};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_contract_gate::{test_wallet, Case, Gate};
use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::handlers::destinations::MAX_BATCH_ITEMS;
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::PlacesComponent;
use catalyrst_places::{api_router_with_spec, AppState, AppStateInner};
use serde_json::{json, Value};
use sqlx::PgPool;

const ADMIN_TOKEN: &str = "cg-places-admin";
const DATA_TOKEN: &str = "cg-places-data";
const PLACE_ID: &str = "123e4567-e89b-12d3-a456-426614174000";
const WORLD_ROW_ID: &str = "world-entity-1";
const WORLD_NAME: &str = "gate-world.dcl.eth";
const WORLD_EVENT_ID: &str = "ev-world-1";
const DEAD_EVENTS_URL: &str = "http://127.0.0.1:9";

// The events service the proxy and the decorations talk to: it hosts one
// live, upcoming event at the world, named the way upstream names a world
// event (its world in `server`, an upstream place uuid on the wire), and
// answers the destination proxy with an empty list.
async fn events_service() -> String {
    let world_event = json!({
        "id": WORLD_EVENT_ID,
        "place_id": "upstream-world-uuid",
        "server": WORLD_NAME,
        "world": true,
        "name": "World Party",
        "next_start_at": "2030-01-01T10:00:00Z"
    });
    let app = Router::new()
        .route(
            "/api/events/search",
            post(move || {
                let reply = json!({ "ok": true, "data": { "events": [world_event.clone()] } });
                async move { Json(reply) }
            }),
        )
        .route(
            "/v1/destinations/{id}/events",
            get(|Path(_id): Path<String>| async { Json(json!({ "ok": true, "data": [] })) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

fn too_many_ids() -> String {
    format!("ids={}", vec!["x"; MAX_BATCH_ITEMS + 1].join(","))
}

async fn fixture_tables(pool: &PgPool) {
    sqlx::query(
        r#"
        CREATE TABLE place (
            id             text PRIMARY KEY,
            title          text,
            description    text,
            creator_address text,
            base_position  text NOT NULL,
            content_rating text,
            disabled       boolean NOT NULL DEFAULT false,
            favorites      integer NOT NULL DEFAULT 0,
            likes          integer NOT NULL DEFAULT 0,
            dislikes       integer NOT NULL DEFAULT 0,
            categories     text[]  NOT NULL DEFAULT '{}',
            highlighted    boolean NOT NULL DEFAULT false,
            deployed_at    timestamptz,
            raw            jsonb   NOT NULL DEFAULT '{}'::jsonb
        )
        "#,
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!("../migrations/0002_place_indexed.sql"))
        .execute(pool)
        .await
        .expect("create place_indexed");

    sqlx::raw_sql(include_str!("../migrations/0003_place_world_name.sql"))
        .execute(pool)
        .await
        .expect("promote world_name");
}

async fn seed_place(pool: &PgPool, id: &str, base: &str, raw: Value) {
    sqlx::query(
        "INSERT INTO place (id, title, base_position, deployed_at, raw) \
         VALUES ($1, $2, $3, now(), $4)",
    )
    .bind(id)
    .bind(format!("place {id}"))
    .bind(base)
    .bind(raw)
    .execute(pool)
    .await
    .unwrap();
}

async fn build_state(pool: PgPool, admin_address: String, events_url: &str) -> AppState {
    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    places.ensure_local_schema().await.unwrap();
    Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool.clone()),
        admin_addresses: vec![admin_address],
        data_team_auth_token: Some(DATA_TOKEN.into()),
        admin_auth_token: Some(ADMIN_TOKEN.into()),
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:9".into()),
        events: Events::new(events_url.into()),
        presence: Presence::new("http://127.0.0.1:9".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    })
}

#[tokio::test]
async fn every_spec_route_answers_its_contract() {
    std::env::set_var("PLACES_REPORT_LOCAL_FALLBACK", "true");
    let Some(scratch) = ScratchSchema::create("CATALYRST_PLACES_TEST_PG", "cg_places").await else {
        return;
    };
    scratch
        .apply_sql(include_str!("../migrations/0001_lists.sql"))
        .await;
    fixture_tables(&scratch.pool).await;
    seed_place(
        &scratch.pool,
        PLACE_ID,
        "0,0",
        json!({ "positions": ["0,0"], "world": false }),
    )
    .await;
    seed_place(
        &scratch.pool,
        WORLD_ROW_ID,
        "0,0",
        json!({ "world": true, "world_name": WORLD_NAME }),
    )
    .await;

    let user = test_wallet(7);
    let admin = test_wallet(21);
    let events_url = events_service().await;
    let (router, spec) = api_router_with_spec();
    let state = build_state(
        scratch.pool.clone(),
        admin.address().to_lowercase(),
        &events_url,
    )
    .await;
    let app: Router = router.with_state(state);
    let dead_state = build_state(
        scratch.pool.clone(),
        admin.address().to_lowercase(),
        DEAD_EVENTS_URL,
    )
    .await;
    let dead_app: Router = api_router_with_spec().0.with_state(dead_state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    gate.hit(&app, Case::new("get", "/api/categories")).await;
    gate.hit(&app, Case::new("get", "/api/status")).await;

    gate.hit(&app, Case::new("get", "/api/places")).await;
    gate.hit(
        &app,
        Case::new("post", "/api/places").json(&json!([PLACE_ID])),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/places")
            .json(&json!({ "ids": [PLACE_ID] }))
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("post", "/api/places/status").json(&json!([PLACE_ID])),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/places/status")
            .json(&json!("nope"))
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/places/{place_id}").path(&format!("/api/places/{}", PLACE_ID)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/api/places/{place_id}")
            .path("/api/places/nope")
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/api/places/{place_id}/categories")
            .path(&format!("/api/places/{}/categories", PLACE_ID)),
    )
    .await;
    gate.waive_error(
        "get",
        "/api/places/{place_id}/categories",
        "unknown places answer 200 with empty categories; the documented 404 is unreachable",
    );

    let favorites_path = format!("/api/places/{}/favorites", PLACE_ID);
    gate.hit(
        &app,
        Case::new("patch", "/api/places/{entity_id}/favorites")
            .path(&favorites_path)
            .signed(&user)
            .json(&json!({ "favorites": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/places/{entity_id}/favorites")
            .path(&favorites_path)
            .json(&json!({ "favorites": true }))
            .expect(401),
    )
    .await;

    let likes_path = format!("/api/places/{}/likes", PLACE_ID);
    gate.hit(
        &app,
        Case::new("patch", "/api/places/{entity_id}/likes")
            .path(&likes_path)
            .signed(&user)
            .json(&json!({ "like": null })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/places/{entity_id}/likes")
            .path(&likes_path)
            .json(&json!({ "like": null }))
            .expect(401),
    )
    .await;

    let disable_path = format!("/api/places/{}/disable", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/disable")
            .path(&disable_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "disabled": false })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/disable")
            .path(&disable_path)
            .json(&json!({ "disabled": false }))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/places/{place_id}/disable")
            .path(&disable_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "disabled": false })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/places/{place_id}/disable")
            .path("/api/places/nope/disable")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "disabled": false }))
            .expect(404),
    )
    .await;

    let featured_path = format!("/api/places/{}/featured", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/featured")
            .path(&featured_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "featured": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/featured")
            .path(&featured_path)
            .json(&json!({ "featured": true }))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/places/{place_id}/featured")
            .path(&featured_path)
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/places/{place_id}/featured")
            .path(&featured_path)
            .expect(401),
    )
    .await;

    let highlight_path = format!("/api/places/{}/highlight", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/highlight")
            .path(&highlight_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "highlighted": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/highlight")
            .path(&highlight_path)
            .json(&json!({ "highlighted": true }))
            .expect(401),
    )
    .await;

    let ranking_path = format!("/api/places/{}/ranking", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/ranking")
            .path(&ranking_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "ranking": 1.5 })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/ranking")
            .path(&ranking_path)
            .bearer(DATA_TOKEN)
            .json(&json!({ "ranking": 1.5 }))
            .expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/ranking")
            .path(&ranking_path)
            .json(&json!({ "ranking": 1.5 }))
            .expect(401),
    )
    .await;

    let rating_path = format!("/api/places/{}/rating", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/rating")
            .path(&rating_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "content_rating": "E" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/places/{place_id}/rating")
            .path(&rating_path)
            .json(&json!({ "content_rating": "E" }))
            .expect(401),
    )
    .await;

    gate.hit(&app, Case::new("get", "/api/worlds")).await;
    gate.hit(&app, Case::new("get", "/api/world_names")).await;
    gate.hit(
        &app,
        Case::new("get", "/api/worlds/{world_id}").path(&format!("/api/worlds/{}", WORLD_NAME)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/api/worlds/{world_id}")
            .path("/api/worlds/nope.dcl.eth")
            .expect(404),
    )
    .await;

    let wfav_path = format!("/api/worlds/{}/favorites", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("patch", "/api/worlds/{world_id}/favorites")
            .path(&wfav_path)
            .signed(&user)
            .json(&json!({ "favorites": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/worlds/{world_id}/favorites")
            .path(&wfav_path)
            .json(&json!({ "favorites": true }))
            .expect(401),
    )
    .await;

    let wlikes_path = format!("/api/worlds/{}/likes", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("patch", "/api/worlds/{world_id}/likes")
            .path(&wlikes_path)
            .signed(&user)
            .json(&json!({ "like": null })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/worlds/{world_id}/likes")
            .path(&wlikes_path)
            .json(&json!({ "like": null }))
            .expect(401),
    )
    .await;

    let wfeat_path = format!("/api/worlds/{}/featured", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/featured")
            .path(&wfeat_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "featured": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/featured")
            .path(&wfeat_path)
            .json(&json!({ "featured": true }))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/worlds/{world_id}/featured")
            .path(&wfeat_path)
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/worlds/{world_id}/featured")
            .path(&wfeat_path)
            .expect(401),
    )
    .await;

    let whigh_path = format!("/api/worlds/{}/highlight", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/highlight")
            .path(&whigh_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "highlighted": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/highlight")
            .path(&whigh_path)
            .json(&json!({ "highlighted": true }))
            .expect(401),
    )
    .await;

    let wrank_path = format!("/api/worlds/{}/ranking", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/ranking")
            .path(&wrank_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "ranking": 2.0 })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/ranking")
            .path(&wrank_path)
            .bearer(DATA_TOKEN)
            .json(&json!({ "ranking": 2.0 }))
            .expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/ranking")
            .path(&wrank_path)
            .json(&json!({ "ranking": 2.0 }))
            .expect(401),
    )
    .await;

    let wrate_path = format!("/api/worlds/{}/rating", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/rating")
            .path(&wrate_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "content_rating": "T" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/worlds/{world_id}/rating")
            .path(&wrate_path)
            .json(&json!({ "content_rating": "T" }))
            .expect(401),
    )
    .await;

    gate.hit(&app, Case::new("get", "/api/map")).await;
    gate.hit(&app, Case::new("get", "/api/map/places")).await;

    gate.hit(&app, Case::new("get", "/v1/destinations")).await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations").query("ids=nope,zzz&kinds=world&limit=x"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations")
            .query(&too_many_ids())
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}")
            .path(&format!("/v1/destinations/{}", PLACE_ID))
            .query("with=live_events,next_event,realms_detail"),
    )
    .await;
    let world = gate
        .hit(
            &app,
            Case::new("get", "/v1/destinations/{id}")
                .path(&format!("/v1/destinations/{}", WORLD_NAME))
                .query("with=live_events,next_event"),
        )
        .await;
    assert_eq!(
        (
            world["data"]["live_event"].clone(),
            world["data"]["live_event_name"].clone(),
            world["data"]["next_event"]["id"].clone(),
        ),
        (json!(true), json!("World Party"), json!(WORLD_EVENT_ID)),
        "a world is decorated with the event its world hosts: {world}"
    );
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}")
            .path("/v1/destinations/00000000-0000-0000-0000-000000000000")
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}")
            .path("/v1/destinations/no-such-world.dcl.eth")
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/destinations/{id}/events")
            .path(&format!("/v1/destinations/{}/events", PLACE_ID)),
    )
    .await;
    gate.hit(
        &dead_app,
        Case::new("get", "/v1/destinations/{id}/events")
            .path(&format!("/v1/destinations/{}/events", PLACE_ID))
            .expect(503),
    )
    .await;

    let v1_favorites_path = format!("/v1/destinations/{}/favorites", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/favorites")
            .path(&v1_favorites_path)
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/favorites")
            .path(&v1_favorites_path)
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/favorites")
            .path("/v1/destinations/00000000-0000-0000-0000-000000000000/favorites")
            .signed(&user)
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/destinations/{id}/favorites")
            .path(&v1_favorites_path)
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/destinations/{id}/favorites")
            .path(&v1_favorites_path)
            .expect(401),
    )
    .await;
    let v1_world_favorites_path = format!("/v1/destinations/{}/favorites", WORLD_NAME);
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/favorites")
            .path(&v1_world_favorites_path)
            .signed(&user),
    )
    .await;
    let own_favorites = gate
        .hit(
            &app,
            Case::new("get", "/v1/destinations")
                .query("only_favorites=true")
                .signed(&user),
        )
        .await;
    let own_ids: Vec<&str> = own_favorites["data"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();
    assert!(
        own_ids.contains(&WORLD_ROW_ID),
        "a signed caller reads their own favorites: {own_favorites}"
    );
    let forged_chain = json!({ "type": "SIGNER", "payload": user.address().to_lowercase() });
    let forged = gate
        .hit(
            &app,
            Case::new("get", "/v1/destinations")
                .query("only_favorites=true")
                .header("x-identity-auth-chain-0", &forged_chain.to_string()),
        )
        .await;
    assert_eq!(
        forged["data"],
        json!([]),
        "an unsigned auth chain naming a wallet must not read its favorites: {forged}"
    );

    let v1_likes_path = format!("/v1/destinations/{}/likes", PLACE_ID);
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/likes")
            .path(&v1_likes_path)
            .signed(&user)
            .json(&json!({ "like": true })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/likes")
            .path(&v1_likes_path)
            .signed(&user)
            .json(&json!({ "like": "yes" }))
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/v1/destinations/{id}/likes")
            .path(&v1_likes_path)
            .json(&json!({ "like": true }))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/destinations/{id}/likes")
            .path(&v1_likes_path)
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/destinations/{id}/likes")
            .path(&v1_likes_path)
            .expect(401),
    )
    .await;

    gate.hit(&app, Case::new("get", "/v1/categories")).await;
    gate.hit(
        &app,
        Case::new("get", "/v1/categories")
            .query("target=events")
            .expect(503),
    )
    .await;

    gate.hit(&app, Case::new("get", "/api/destinations")).await;
    gate.hit(
        &app,
        Case::new("get", "/api/destinations")
            .query(&too_many_ids())
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/destinations").json(&json!([PLACE_ID])),
    )
    .await;
    let nothing = gate
        .hit(
            &app,
            Case::new("post", "/api/destinations").json(&json!({ "ids": [] })),
        )
        .await;
    assert_eq!(
        (nothing["data"].clone(), nothing["total"].clone()),
        (json!([]), json!(0)),
        "a by-id lookup that names nothing answers nothing, as upstream's does"
    );
    gate.hit(
        &app,
        Case::new("post", "/api/destinations")
            .json(&json!(vec![PLACE_ID; MAX_BATCH_ITEMS + 1]))
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("post", "/api/report")
            .path("/api/report")
            .signed(&user)
            .header("host", "gate.test")
            .json(&json!({ "entity_id": PLACE_ID, "reason": "contract gate" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/report")
            .json(&json!({ "entity_id": PLACE_ID }))
            .expect(401),
    )
    .await;

    let reports = gate
        .hit(&app, Case::new("get", "/api/reports").bearer(ADMIN_TOKEN))
        .await;
    let report_id = reports["data"][0]["id"].as_i64().unwrap();
    gate.hit(&app, Case::new("get", "/api/reports").expect(403))
        .await;

    let filename = reports["data"][0]["filename"].as_str().unwrap().to_string();
    let upload_path = format!("/api/report/upload/{}", filename);
    gate.hit(
        &app,
        Case::new("put", "/api/report/upload/{filename}")
            .path(&upload_path)
            .signed(&user)
            .json(&json!({ "detail": "uploaded" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/report/upload/{filename}")
            .path(&upload_path)
            .json(&json!({ "detail": "anonymous" }))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("put", "/api/report/upload/{filename}")
            .path("/api/report/upload/not-my-report.json")
            .signed(&user)
            .json(&json!({ "detail": "someone else" }))
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("patch", "/api/reports/{id}")
            .path(&format!("/api/reports/{}", report_id))
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "status": "resolved" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/reports/{id}")
            .path("/api/reports/999999")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "status": "resolved" }))
            .expect(404),
    )
    .await;

    gate.hit(&app, Case::new("get", "/api/pois").bearer(ADMIN_TOKEN))
        .await;
    gate.hit(
        &app,
        Case::new("post", "/api/pois")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "position": "5,5", "title": "Gate POI" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/api/pois")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "position": "  " }))
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/pois/{position}")
            .path("/api/pois/5,5")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "title": "Gate POI 2" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("patch", "/api/pois/{position}")
            .path("/api/pois/9,9")
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "title": "missing" }))
            .expect(404),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/pois/{position}")
            .path("/api/pois/5,5")
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/api/pois/{position}")
            .path("/api/pois/5,5")
            .bearer(ADMIN_TOKEN)
            .expect(404),
    )
    .await;

    gate.hit(&app, Case::new("get", "/federation/places/snapshot"))
        .await;
    gate.hit(&app, Case::new("get", "/federation/places/changes"))
        .await;

    gate.hit(
        &app,
        Case::new("get", "/places/place").query("position=0,0"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/places/world").query(&format!("name={}", WORLD_NAME)),
    )
    .await;

    gate.assert_covered();

    scratch.drop().await;
}
