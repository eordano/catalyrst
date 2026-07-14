use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::handlers::ranking_exclusion::put_world_ranking_exclusion;
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::{PlaceListFilters, PlacesComponent, ScoreRankingOutcome};
use catalyrst_places::{AppState, AppStateInner};

const ADMIN_TOKEN: &str = "rankexcl-admin";

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

async fn seed(pool: &PgPool, id: &str, highlighted: bool, ranking: f64) {
    sqlx::query("INSERT INTO place (id, base_position, highlighted, raw) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("0,0")
        .bind(highlighted)
        .bind(serde_json::json!({ "ranking": ranking, "like_score": 0.5 }))
        .execute(pool)
        .await
        .expect("seed place");
}

async fn ranking_of(places: &PlacesComponent, id: &str) -> (Option<f64>, bool) {
    let row = places
        .find_by_id(id)
        .await
        .expect("read place")
        .expect("place exists");
    (row.ranking, row.exclude_from_ranking)
}

#[tokio::test]
async fn the_score_is_refused_once_the_destination_became_editorial() {
    let Some(scratch) = setup("cg_places_rankexcl").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed(&pool, "plain", false, 3.0).await;
    seed(&pool, "shelf", true, 7.0).await;

    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());

    assert_eq!(
        places
            .set_ranking_from_score("plain", Some(5.0))
            .await
            .expect("score write"),
        ScoreRankingOutcome::Stored,
        "an uncurated place still takes the automated score"
    );
    assert_eq!(ranking_of(&places, "plain").await, (Some(5.0), false));

    assert_eq!(
        places
            .set_ranking_from_score("shelf", Some(5.0))
            .await
            .expect("score write"),
        ScoreRankingOutcome::RefusedCurated,
        "a highlighted place refuses the automated score in the write itself"
    );
    assert_eq!(ranking_of(&places, "shelf").await, (Some(7.0), false));

    assert_eq!(
        places
            .set_exclude_from_ranking("plain", true)
            .await
            .expect("exclude"),
        1
    );
    assert_eq!(
        ranking_of(&places, "plain").await,
        (Some(0.0), true),
        "excluding clears the score's last word in the same statement"
    );

    assert_eq!(
        places
            .set_ranking_from_score("plain", Some(9.0))
            .await
            .expect("score write"),
        ScoreRankingOutcome::RefusedCurated,
        "the exclusion closes the window the read-then-write left open"
    );
    assert_eq!(ranking_of(&places, "plain").await, (Some(0.0), true));

    places
        .set_ranking("plain", Some(9.0))
        .await
        .expect("admin write");
    assert_eq!(
        ranking_of(&places, "plain").await,
        (Some(9.0), true),
        "the admin may still set a ranking by hand on an excluded place"
    );

    assert_eq!(
        places
            .set_exclude_from_ranking("plain", false)
            .await
            .expect("include"),
        1
    );
    assert_eq!(
        ranking_of(&places, "plain").await,
        (Some(9.0), false),
        "clearing the flag leaves the last known ranking alone"
    );
    assert_eq!(
        places
            .set_ranking_from_score("plain", Some(1.0))
            .await
            .expect("score write"),
        ScoreRankingOutcome::Stored
    );

    assert_eq!(
        places
            .set_ranking_from_score("no-such-place", Some(1.0))
            .await
            .expect("score write"),
        ScoreRankingOutcome::Stored,
        "a row this leg cannot write is a no-op, not an editorial refusal"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn only_excluded_from_ranking_narrows_the_listing() {
    let Some(scratch) = setup("cg_places_rankexcl_list").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed(&pool, "kept", false, 1.0).await;
    seed(&pool, "skipped", false, 2.0).await;

    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    places
        .set_exclude_from_ranking("skipped", true)
        .await
        .expect("exclude");

    let all = places
        .find_list(&PlaceListFilters {
            limit: 100,
            order_desc: true,
            ..Default::default()
        })
        .await
        .expect("list");
    assert_eq!(
        all.len(),
        2,
        "the flag never hides a destination from browse"
    );

    let filters = PlaceListFilters {
        limit: 100,
        order_desc: true,
        only_excluded_from_ranking: true,
        ..Default::default()
    };
    let only: Vec<String> = places
        .find_list(&filters)
        .await
        .expect("list")
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(only, vec!["skipped".to_string()]);
    assert_eq!(places.count_list(&filters).await.expect("count"), 1);

    scratch.drop().await;
}

async fn seed_raw(pool: &PgPool, id: &str, raw: serde_json::Value) {
    sqlx::query("INSERT INTO place (id, base_position, raw) VALUES ($1, '0,0', $2)")
        .bind(id)
        .bind(raw)
        .execute(pool)
        .await
        .expect("seed raw place");
}

// `raw` carries third-party JSON, so the flag's reader must survive a value
// that is not a boolean at all: casting one aborts the whole listing, not the
// row that holds it.
#[tokio::test]
async fn a_junk_exclusion_flag_reads_as_false_instead_of_failing_the_listing() {
    let Some(scratch) = setup("cg_places_rankexcl_junk").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_raw(
        &pool,
        "empty-string",
        serde_json::json!({ "exclude_from_ranking": "" }),
    )
    .await;
    seed_raw(
        &pool,
        "a-word",
        serde_json::json!({ "exclude_from_ranking": "later" }),
    )
    .await;
    seed_raw(
        &pool,
        "a-number",
        serde_json::json!({ "exclude_from_ranking": 3 }),
    )
    .await;
    seed_raw(
        &pool,
        "really-excluded",
        serde_json::json!({ "exclude_from_ranking": true }),
    )
    .await;

    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    let filters = PlaceListFilters {
        limit: 100,
        order_desc: true,
        ..Default::default()
    };
    let mut listed: Vec<(String, bool)> = places
        .find_list(&filters)
        .await
        .expect("list")
        .into_iter()
        .map(|r| (r.id, r.exclude_from_ranking))
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        vec![
            ("a-number".to_string(), false),
            ("a-word".to_string(), false),
            ("empty-string".to_string(), false),
            ("really-excluded".to_string(), true),
        ]
    );
    assert_eq!(places.count_list(&filters).await.expect("count"), 4);

    let only = PlaceListFilters {
        only_excluded_from_ranking: true,
        ..filters
    };
    let ids: Vec<String> = places
        .find_list(&only)
        .await
        .expect("list")
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, vec!["really-excluded".to_string()]);

    assert_eq!(
        places
            .set_ranking_from_score("empty-string", Some(4.0))
            .await
            .expect("score write"),
        ScoreRankingOutcome::Stored,
        "a junk flag is not curation, so the score still lands"
    );
    assert_eq!(
        ranking_of(&places, "empty-string").await,
        (Some(4.0), false)
    );

    scratch.drop().await;
}

fn state_for(pool: &PgPool) -> AppState {
    Arc::new(AppStateInner {
        places: PlacesComponent::new(pool.clone()).with_writer(pool.clone()),
        lists: ListsComponent::new(pool.clone()),
        admin_addresses: Vec::new(),
        data_team_auth_token: None,
        admin_auth_token: Some(ADMIN_TOKEN.into()),
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:9".into()),
        events: Events::new("http://127.0.0.1:9".into()),
        presence: Presence::new("http://127.0.0.1:9".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    })
}

fn admin_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {ADMIN_TOKEN}")).unwrap(),
    );
    headers
}

// place_indexed reads both legs, the write reaches `place` alone: a world our
// own catalyrst-worlds serves lives in place_world_local, so the route must
// refuse rather than answer 200 with a flag nothing stored.
#[tokio::test]
async fn a_locally_served_world_is_never_reported_as_excluded() {
    let Some(scratch) = setup("cg_places_rankexcl_local").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, world, world_name, raw) \
         VALUES ($1, '0,0', true, $2, $3)",
    )
    .bind("local-world-1")
    .bind("mine.dcl.eth")
    .bind(serde_json::json!({ "world": true, "world_name": "mine.dcl.eth" }))
    .execute(&pool)
    .await
    .expect("seed local world");

    let state = state_for(&pool);
    let found = state
        .places
        .find_world_by_id("mine.dcl.eth")
        .await
        .expect("read world")
        .expect("the world is browsable");
    assert!(!found.exclude_from_ranking);

    let status = put_world_ranking_exclusion(
        State(state.clone()),
        admin_headers(),
        Path("mine.dcl.eth".to_string()),
    )
    .await
    .expect_err("a write that cannot land is not a success")
    .into_response()
    .status();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    assert!(
        !state
            .places
            .find_world_by_id("mine.dcl.eth")
            .await
            .expect("read world")
            .expect("still browsable")
            .exclude_from_ranking,
        "nothing was stored, so nothing reads back as excluded"
    );

    scratch.drop().await;
}
