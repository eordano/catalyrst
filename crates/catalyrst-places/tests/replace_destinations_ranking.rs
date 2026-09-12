use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use catalyrst_contract_gate::pg::ScratchSchema;
use serde_json::{json, Value};
use sqlx::PgPool;

use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::handlers::federation::{put_place_ranking, put_world_ranking};
use catalyrst_places::handlers::replace_ranking::put_destinations_ranking;
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::{PlaceListFilters, PlacesComponent, ReplaceRankingResult};
use catalyrst_places::{AppState, AppStateInner};

const DATA_TEAM_TOKEN: &str = "replace-ranking-data-team";
const ADMIN_TOKEN: &str = "replace-ranking-admin";

async fn setup(schema: &str) -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        schema,
    )
    .await
}

async fn create_place_table(pool: &PgPool) {
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
    .expect("create place table");

    sqlx::raw_sql(include_str!("../migrations/0002_place_indexed.sql"))
        .execute(pool)
        .await
        .expect("create place_indexed");

    sqlx::raw_sql(include_str!("../migrations/0003_place_world_name.sql"))
        .execute(pool)
        .await
        .expect("promote world_name");
}

async fn seed_place(pool: &PgPool, id: &str, highlighted: bool, raw: Value) {
    sqlx::query(
        "INSERT INTO place (id, base_position, highlighted, raw) VALUES ($1, '0,0', $2, $3)",
    )
    .bind(id)
    .bind(highlighted)
    .bind(raw)
    .execute(pool)
    .await
    .expect("seed place");
}

async fn seed_world(pool: &PgPool, id: &str, name: &str, highlighted: bool, raw: Value) {
    let mut raw = raw;
    raw["world"] = json!(true);
    raw["world_name"] = json!(name);
    seed_place(pool, id, highlighted, raw).await;
}

async fn seed_local_world(pool: &PgPool, id: &str, name: &str) {
    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, world, world_name, raw) \
         VALUES ($1, '0,0', true, $2, $3)",
    )
    .bind(id)
    .bind(name)
    .bind(json!({ "world": true, "world_name": name }))
    .execute(pool)
    .await
    .expect("seed local world");
}

fn state_for(pool: &PgPool) -> AppState {
    state_from(
        PlacesComponent::new(pool.clone()).with_writer(pool.clone()),
        pool,
    )
}

fn state_without_writer(pool: &PgPool) -> AppState {
    state_from(PlacesComponent::new(pool.clone()), pool)
}

fn state_from(places: PlacesComponent, pool: &PgPool) -> AppState {
    Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool.clone()),
        admin_addresses: Vec::new(),
        data_team_auth_token: Some(DATA_TEAM_TOKEN.into()),
        admin_auth_token: Some(ADMIN_TOKEN.into()),
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:9".into()),
        events: Events::new("http://127.0.0.1:9".into()),
        presence: Presence::new("http://127.0.0.1:9".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    })
}

fn bearer(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    headers
}

async fn replace_as(state: &AppState, token: &str, entries: Value) -> ReplaceRankingResult {
    let (status, Json(body)) = put_destinations_ranking(
        State(state.clone()),
        bearer(token),
        Some(Json(json!({ "entries": entries }))),
    )
    .await
    .expect("the replace ran");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a completed run answers 201 Created"
    );
    body.data
}

async fn replace(state: &AppState, entries: Value) -> ReplaceRankingResult {
    replace_as(state, DATA_TEAM_TOKEN, entries).await
}

async fn rejected(state: &AppState, entries: Value) -> StatusCode {
    put_destinations_ranking(
        State(state.clone()),
        bearer(DATA_TEAM_TOKEN),
        Some(Json(json!({ "entries": entries }))),
    )
    .await
    .expect_err("the payload is refused")
    .into_response()
    .status()
}

async fn ranking_of_place(state: &AppState, id: &str) -> Option<f64> {
    state
        .places
        .find_by_id(id)
        .await
        .expect("read place")
        .expect("the place exists")
        .ranking
}

async fn ranking_of_world(state: &AppState, id: &str) -> Option<f64> {
    state
        .places
        .find_world_by_id(id)
        .await
        .expect("read world")
        .expect("the world exists")
        .ranking
}

#[tokio::test]
async fn a_run_without_a_bearer_token_writes_nothing() {
    let Some(scratch) = setup("cg_places_replace_authz").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "plain", false, json!({ "ranking": 5 })).await;
    let state = state_for(&pool);

    for headers in [HeaderMap::new(), bearer("not-the-token")] {
        let status = put_destinations_ranking(
            State(state.clone()),
            headers,
            Some(Json(json!({ "entries": [] }))),
        )
        .await
        .expect_err("an unauthorized run is refused")
        .into_response()
        .status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(ranking_of_place(&state, "plain").await, Some(5.0));

    scratch.drop().await;
}

#[tokio::test]
async fn the_token_is_checked_before_the_body() {
    let Some(scratch) = setup("cg_places_replace_authz_order").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    let state = state_for(&pool);

    let status = put_destinations_ranking(
        State(state.clone()),
        HeaderMap::new(),
        Some(Json(json!({ "nonsense": true }))),
    )
    .await
    .expect_err("an unauthorized run is refused")
    .into_response()
    .status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    scratch.drop().await;
}

#[tokio::test]
async fn a_destination_named_twice_rejects_the_whole_run() {
    let Some(scratch) = setup("cg_places_replace_duplicate").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "plain", false, json!({ "ranking": 5 })).await;
    seed_place(&pool, "other", false, json!({ "ranking": 6 })).await;
    let state = state_for(&pool);

    let status = rejected(
        &state,
        json!([
            { "entity_type": "place", "id": "plain", "ranking": 10 },
            { "entity_type": "place", "id": "plain", "ranking": 20 },
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(ranking_of_place(&state, "plain").await, Some(5.0));
    assert_eq!(
        ranking_of_place(&state, "other").await,
        Some(6.0),
        "a refused run never reaches the clear either"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_negative_ranking_rejects_the_whole_run() {
    let Some(scratch) = setup("cg_places_replace_negative").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "plain", false, json!({ "ranking": 5 })).await;
    let state = state_for(&pool);

    let status = rejected(
        &state,
        json!([{ "entity_type": "place", "id": "plain", "ranking": -1 }]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(ranking_of_place(&state, "plain").await, Some(5.0));

    scratch.drop().await;
}

#[tokio::test]
async fn the_run_writes_what_it_names_and_clears_what_it_omits() {
    let Some(scratch) = setup("cg_places_replace_set").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "named", false, json!({ "ranking": 0 })).await;
    seed_place(&pool, "omitted", false, json!({ "ranking": 177 })).await;
    seed_place(&pool, "never-ranked", false, json!({})).await;
    seed_world(&pool, "w-named", "named.dcl.eth", false, json!({})).await;
    seed_world(
        &pool,
        "w-omitted",
        "omitted.dcl.eth",
        false,
        json!({ "ranking": 190 }),
    )
    .await;
    let state = state_for(&pool);

    let result = replace(
        &state,
        json!([
            { "entity_type": "place", "id": "named", "ranking": 122 },
            { "entity_type": "world", "id": "named.dcl.eth", "ranking": 121 },
        ]),
    )
    .await;

    assert_eq!(ranking_of_place(&state, "named").await, Some(122.0));
    assert_eq!(ranking_of_world(&state, "named.dcl.eth").await, Some(121.0));
    assert_eq!(
        ranking_of_place(&state, "omitted").await,
        Some(0.0),
        "a destination that dropped out of the eligible set loses its number"
    );
    assert_eq!(ranking_of_world(&state, "omitted.dcl.eth").await, Some(0.0));

    assert_eq!(result.places.applied, 1);
    assert_eq!(
        result.places.cleared, 1,
        "a row that never carried a ranking is not rewritten"
    );
    assert_eq!(result.worlds.applied, 1);
    assert_eq!(result.worlds.cleared, 1);
    assert!(result.places.skipped_missing.is_empty());
    assert!(result.worlds.skipped_missing.is_empty());

    let ids: Vec<String> = state
        .places
        .find_list(&PlaceListFilters {
            limit: 100,
            order_desc: true,
            destinations_mode: true,
            only_places: true,
            ..Default::default()
        })
        .await
        .expect("destinations list")
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(
        ids,
        vec![
            "named".to_string(),
            "never-ranked".to_string(),
            "omitted".to_string()
        ],
        "the cleared row must tie with the never-ranked one, not sit above it"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_node_without_a_writer_refuses_the_run() {
    let Some(scratch) = setup("cg_places_replace_no_writer").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "plain", false, json!({ "ranking": 5 })).await;
    let state = state_without_writer(&pool);

    let status = put_destinations_ranking(
        State(state.clone()),
        bearer(DATA_TEAM_TOKEN),
        Some(Json(
            json!({ "entries": [{ "entity_type": "place", "id": "plain", "ranking": 10 }] }),
        )),
    )
    .await
    .expect_err("a run with nowhere to write is not a success")
    .into_response()
    .status();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(ranking_of_place(&state, "plain").await, Some(5.0));

    scratch.drop().await;
}

#[tokio::test]
async fn two_casings_of_one_world_name_reject_the_whole_run() {
    let Some(scratch) = setup("cg_places_replace_casing").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_world(
        &pool,
        "w-named",
        "Named.dcl.eth",
        false,
        json!({ "ranking": 11 }),
    )
    .await;
    seed_world(
        &pool,
        "w-omitted",
        "omitted.dcl.eth",
        false,
        json!({ "ranking": 190 }),
    )
    .await;
    let state = state_for(&pool);

    let status = rejected(
        &state,
        json!([
            { "entity_type": "world", "id": "Named.dcl.eth", "ranking": 10 },
            { "entity_type": "world", "id": "named.dcl.eth", "ranking": 20 },
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(
        ranking_of_world(&state, "Named.dcl.eth").await,
        Some(11.0),
        "a refused run writes nothing"
    );
    assert_eq!(
        ranking_of_world(&state, "omitted.dcl.eth").await,
        Some(190.0),
        "a refused run never reaches the clear either"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn a_curated_destination_survives_the_clear() {
    let Some(scratch) = setup("cg_places_replace_curated_omitted").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "shelf", true, json!({ "ranking": 1900 })).await;
    seed_place(
        &pool,
        "excluded",
        false,
        json!({ "ranking": 42, "exclude_from_ranking": true }),
    )
    .await;
    seed_world(
        &pool,
        "w-shelf",
        "curatedworld.dcl.eth",
        true,
        json!({ "ranking": 1800 }),
    )
    .await;
    let state = state_for(&pool);

    let result = replace(&state, json!([])).await;

    assert_eq!(ranking_of_place(&state, "shelf").await, Some(1900.0));
    assert_eq!(ranking_of_place(&state, "excluded").await, Some(42.0));
    assert_eq!(
        ranking_of_world(&state, "curatedworld.dcl.eth").await,
        Some(1800.0)
    );
    assert_eq!(result.places.cleared, 0);
    assert_eq!(result.worlds.cleared, 0);

    scratch.drop().await;
}

#[tokio::test]
async fn a_named_curated_destination_is_refused_under_either_token() {
    let Some(scratch) = setup("cg_places_replace_curated_named").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "shelf", true, json!({ "ranking": 1900 })).await;
    seed_world(
        &pool,
        "w-excluded",
        "excludedworld.dcl.eth",
        false,
        json!({ "ranking": 12, "exclude_from_ranking": true }),
    )
    .await;
    let state = state_for(&pool);

    for token in [DATA_TEAM_TOKEN, ADMIN_TOKEN] {
        let result = replace_as(
            &state,
            token,
            json!([
                { "entity_type": "place", "id": "shelf", "ranking": 7 },
                { "entity_type": "world", "id": "excludedworld.dcl.eth", "ranking": 8 },
            ]),
        )
        .await;
        assert_eq!(result.places.skipped_curated, vec!["shelf".to_string()]);
        assert_eq!(result.places.applied, 0);
        assert_eq!(
            result.worlds.skipped_curated,
            vec!["excludedworld.dcl.eth".to_string()]
        );
        assert_eq!(result.worlds.applied, 0);
        assert_eq!(ranking_of_place(&state, "shelf").await, Some(1900.0));
        assert_eq!(
            ranking_of_world(&state, "excludedworld.dcl.eth").await,
            Some(12.0)
        );
    }

    scratch.drop().await;
}

#[tokio::test]
async fn a_world_row_addressed_as_a_place_is_refused_and_reported() {
    let Some(scratch) = setup("cg_places_replace_world_backed").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_world(
        &pool,
        "w-inner",
        "innerworld.dcl.eth",
        false,
        json!({ "ranking": 24 }),
    )
    .await;
    seed_world(
        &pool,
        "w-both",
        "bothworld.dcl.eth",
        true,
        json!({ "ranking": 1900 }),
    )
    .await;
    let state = state_for(&pool);

    let result = replace(
        &state,
        json!([
            { "entity_type": "place", "id": "w-inner", "ranking": 30 },
            { "entity_type": "place", "id": "w-both", "ranking": 5 },
        ]),
    )
    .await;

    assert_eq!(
        result.places.skipped_world_backed,
        vec!["w-inner".to_string()],
        "a world row is never ranked through the places leg"
    );
    assert_eq!(
        result.places.skipped_curated,
        vec!["w-both".to_string()],
        "curation wins the tie so the skip counts still add up"
    );
    assert_eq!(result.places.applied, 0);
    assert_eq!(
        ranking_of_world(&state, "innerworld.dcl.eth").await,
        Some(0.0),
        "the worlds leg still clears the stale value it was not named for"
    );
    assert_eq!(
        (result.places.cleared, result.worlds.cleared),
        (0, 1),
        "one table holds both legs, so the row upstream counts under \
         places.cleared is counted here by the worlds leg"
    );
    assert_eq!(
        ranking_of_world(&state, "bothworld.dcl.eth").await,
        Some(1900.0)
    );

    scratch.drop().await;
}

#[tokio::test]
async fn an_unknown_destination_is_reported_rather_than_failing_the_run() {
    let Some(scratch) = setup("cg_places_replace_missing").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "plain", false, json!({ "ranking": 3 })).await;
    let state = state_for(&pool);

    let result = replace(
        &state,
        json!([
            { "entity_type": "place", "id": "plain", "ranking": 10 },
            { "entity_type": "place", "id": "absent", "ranking": 9 },
            { "entity_type": "world", "id": "absent.dcl.eth", "ranking": 9 },
        ]),
    )
    .await;

    assert_eq!(result.places.skipped_missing, vec!["absent".to_string()]);
    assert_eq!(
        result.worlds.skipped_missing,
        vec!["absent.dcl.eth".to_string()]
    );
    assert_eq!(result.places.applied, 1);
    assert_eq!(ranking_of_place(&state, "plain").await, Some(10.0));

    scratch.drop().await;
}

#[tokio::test]
async fn a_locally_served_world_is_never_counted_as_written() {
    let Some(scratch) = setup("cg_places_replace_local").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_local_world(&pool, "local-world-1", "mine.dcl.eth").await;
    let state = state_for(&pool);

    let result = replace(
        &state,
        json!([
            { "entity_type": "world", "id": "mine.dcl.eth", "ranking": 50 },
            { "entity_type": "place", "id": "local-world-1", "ranking": 50 },
        ]),
    )
    .await;

    assert_eq!(result.worlds.applied, 0);
    assert_eq!(
        result.worlds.skipped_missing,
        vec!["mine.dcl.eth".to_string()]
    );
    assert_eq!(result.places.applied, 0);
    assert_eq!(
        result.places.skipped_world_backed,
        vec!["local-world-1".to_string()]
    );
    assert_eq!(
        ranking_of_world(&state, "mine.dcl.eth").await,
        None,
        "nothing was stored, so nothing reads back as ranked"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn the_single_destination_ranking_route_refuses_an_unwritable_world() {
    let Some(scratch) = setup("cg_places_replace_single").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_local_world(&pool, "local-world-2", "mine.dcl.eth").await;
    let state = state_for(&pool);

    for token in [ADMIN_TOKEN, DATA_TEAM_TOKEN] {
        let status = put_world_ranking(
            State(state.clone()),
            bearer(token),
            Path("mine.dcl.eth".to_string()),
            Some(Json(json!({ "ranking": 7 }))),
        )
        .await
        .expect_err("a write that cannot land is not a success")
        .into_response()
        .status();
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "the same unwritable destination answers the same way to either token"
        );
        assert_eq!(ranking_of_world(&state, "mine.dcl.eth").await, None);
    }

    scratch.drop().await;
}

#[tokio::test]
async fn the_single_destination_routes_refuse_a_node_without_a_writer() {
    let Some(scratch) = setup("cg_places_single_no_writer").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_place(&pool, "plain", false, json!({ "ranking": 5 })).await;
    seed_world(
        &pool,
        "w-named",
        "named.dcl.eth",
        false,
        json!({ "ranking": 11 }),
    )
    .await;
    let state = state_without_writer(&pool);

    for token in [ADMIN_TOKEN, DATA_TEAM_TOKEN] {
        let status = put_place_ranking(
            State(state.clone()),
            bearer(token),
            Path("plain".to_string()),
            Some(Json(json!({ "ranking": 7 }))),
        )
        .await
        .expect_err("a write with nowhere to land is not a success")
        .into_response()
        .status();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

        let status = put_world_ranking(
            State(state.clone()),
            bearer(token),
            Path("named.dcl.eth".to_string()),
            Some(Json(json!({ "ranking": 7 }))),
        )
        .await
        .expect_err("a write with nowhere to land is not a success")
        .into_response()
        .status();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }
    assert_eq!(ranking_of_place(&state, "plain").await, Some(5.0));
    assert_eq!(ranking_of_world(&state, "named.dcl.eth").await, Some(11.0));

    scratch.drop().await;
}
