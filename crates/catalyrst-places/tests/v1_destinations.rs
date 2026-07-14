use std::sync::Arc;

use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::entity_id::EntityType;
use catalyrst_places::handlers::destinations::Destination;
use catalyrst_places::handlers::v1::{
    find_destination, set_destination_favorite, set_destination_like,
};
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::{PlaceListFilters, PlacesComponent};
use catalyrst_places::{AppState, AppStateInner};
use sqlx::PgPool;

const PLACE_ID: &str = "123e4567-e89b-12d3-a456-426614174000";
const DISABLED_PLACE_ID: &str = "123e4567-e89b-12d3-a456-426614174001";
const WORLD_ROW_ID: &str = "world-entity-1";
const WORLD_NAME: &str = "My-World.dcl.eth";
const HIDDEN_WORLD_ROW_ID: &str = "world-entity-hidden";
const HIDDEN_WORLD_NAME: &str = "hidden-world.dcl.eth";
const USER: &str = "0x000000000000000000000000000000000000dead";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_v1dest",
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

async fn seed(pool: &PgPool, id: &str, disabled: bool, raw: serde_json::Value) {
    sqlx::query(
        "INSERT INTO place (id, title, base_position, disabled, deployed_at, raw) \
         VALUES ($1, $2, $3, $4, now(), $5)",
    )
    .bind(id)
    .bind(format!("place {id}"))
    .bind("0,0")
    .bind(disabled)
    .bind(raw)
    .execute(pool)
    .await
    .expect("seed place");
}

async fn seed_all(pool: &PgPool) {
    seed(
        pool,
        PLACE_ID,
        false,
        serde_json::json!({ "positions": ["0,0"], "world": false, "image": "https://x/y.png" }),
    )
    .await;
    seed(
        pool,
        DISABLED_PLACE_ID,
        true,
        serde_json::json!({ "positions": ["1,1"], "world": false }),
    )
    .await;
    seed(
        pool,
        WORLD_ROW_ID,
        false,
        serde_json::json!({ "world": true, "world_name": WORLD_NAME }),
    )
    .await;
    seed(
        pool,
        HIDDEN_WORLD_ROW_ID,
        false,
        serde_json::json!({
            "world": true,
            "world_name": HIDDEN_WORLD_NAME,
            "show_in_places": false
        }),
    )
    .await;
}

fn state(pool: PgPool) -> AppState {
    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool.clone()),
        admin_addresses: vec![],
        data_team_auth_token: None,
        admin_auth_token: None,
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:9".into()),
        events: Events::new("http://127.0.0.1:9".into()),
        presence: Presence::new("http://127.0.0.1:9".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    })
}

#[tokio::test]
async fn a_destination_resolves_by_place_uuid_or_world_name() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_all(&pool).await;
    let places = PlacesComponent::new(pool.clone());

    let place = find_destination(&places, PLACE_ID)
        .await
        .expect("lookup ok")
        .expect("the place resolves");
    let place = Destination::from(place);
    assert_eq!(place.id, PLACE_ID);
    assert_eq!(place.kind, EntityType::Place);
    assert!(!place.world);

    let upper = find_destination(&places, &PLACE_ID.to_ascii_uppercase())
        .await
        .expect("lookup ok")
        .expect("uuid lookups are case-insensitive");
    assert_eq!(upper.id, PLACE_ID);

    let world = find_destination(&places, &WORLD_NAME.to_lowercase())
        .await
        .expect("lookup ok")
        .expect("the world resolves by its name");
    let world = Destination::from(world);
    assert_eq!(world.id, WORLD_ROW_ID);
    assert_eq!(world.kind, EntityType::World);
    assert_eq!(world.world_name.as_deref(), Some(WORLD_NAME));

    let by_row_id = find_destination(&places, WORLD_ROW_ID)
        .await
        .expect("lookup ok")
        .expect("the world resolves by the id the list hands out");
    assert_eq!(by_row_id.id, WORLD_ROW_ID);

    assert!(
        find_destination(&places, DISABLED_PLACE_ID)
            .await
            .expect("lookup ok")
            .is_none(),
        "a disabled place is not a destination"
    );
    assert!(
        find_destination(&places, "00000000-0000-0000-0000-000000000000")
            .await
            .expect("lookup ok")
            .is_none()
    );
    assert!(find_destination(&places, "no-such-world.dcl.eth")
        .await
        .expect("lookup ok")
        .is_none());

    scratch.drop().await;
}

#[tokio::test]
async fn a_world_that_opted_out_of_the_directory_is_neither_listed_nor_resolved() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_all(&pool).await;
    let places = PlacesComponent::new(pool.clone());

    for id in [HIDDEN_WORLD_NAME, HIDDEN_WORLD_ROW_ID] {
        assert!(
            find_destination(&places, id)
                .await
                .expect("lookup ok")
                .is_none(),
            "{id} is not a destination"
        );
    }
    assert!(
        places
            .find_world_by_id(HIDDEN_WORLD_NAME)
            .await
            .expect("lookup ok")
            .is_some(),
        "the legacy by-id world read still resolves it, as upstream's does"
    );

    let worlds = PlaceListFilters {
        limit: 100,
        only_worlds: true,
        destinations_mode: true,
        ..Default::default()
    };
    let ids: Vec<String> = places
        .find_list(&worlds)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![WORLD_ROW_ID.to_string()]);
    assert_eq!(places.count_list(&worlds).await.unwrap(), 1);
    assert_eq!(
        places.world_names().await.unwrap(),
        vec![WORLD_NAME.to_string()]
    );

    scratch.drop().await;
}

#[tokio::test]
async fn favorites_and_likes_answer_the_full_interaction_summary() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_all(&pool).await;
    let state = state(pool.clone());
    state.places.ensure_local_schema().await.unwrap();

    let s = set_destination_favorite(&state, PLACE_ID, USER, true)
        .await
        .unwrap();
    assert!(s.user_favorite);
    assert_eq!(s.favorites, 1);
    assert_eq!(
        (s.likes, s.dislikes, s.user_like, s.user_dislike),
        (0, 0, false, false)
    );

    let again = set_destination_favorite(&state, PLACE_ID, USER, true)
        .await
        .unwrap();
    assert_eq!(again.favorites, 1, "a re-favorite is idempotent");

    let upper = set_destination_favorite(&state, &PLACE_ID.to_ascii_uppercase(), USER, true)
        .await
        .expect("a place resolves whatever the case of its uuid");
    assert_eq!(upper.favorites, 1);
    assert!(upper.user_favorite);

    let cleared = set_destination_favorite(&state, PLACE_ID, USER, false)
        .await
        .unwrap();
    assert!(!cleared.user_favorite);
    assert_eq!(cleared.favorites, 0);

    let liked = set_destination_like(&state, PLACE_ID, USER, Some(true))
        .await
        .unwrap();
    assert!(liked.user_like && !liked.user_dislike);
    assert_eq!(liked.likes, 1);

    let disliked = set_destination_like(&state, PLACE_ID, USER, Some(false))
        .await
        .unwrap();
    assert!(!disliked.user_like && disliked.user_dislike);
    assert_eq!((disliked.likes, disliked.dislikes), (0, 1));

    let cleared = set_destination_like(&state, PLACE_ID, USER, None)
        .await
        .unwrap();
    assert!(!cleared.user_like && !cleared.user_dislike);
    assert_eq!((cleared.likes, cleared.dislikes), (0, 0));

    let world = set_destination_favorite(&state, &WORLD_NAME.to_lowercase(), USER, true)
        .await
        .unwrap();
    assert!(world.user_favorite);
    assert_eq!(world.favorites, 1);

    let missing =
        set_destination_favorite(&state, "00000000-0000-0000-0000-000000000000", USER, true)
            .await
            .expect_err("an unknown destination is not found");
    assert!(format!("{missing:?}").contains("404") || format!("{missing}").contains("Not found"));

    scratch.drop().await;
}

#[tokio::test]
async fn a_parcel_and_a_world_name_together_select_both_kinds() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;
    seed_all(&pool).await;
    let places = PlacesComponent::new(pool.clone());

    let both = PlaceListFilters {
        limit: 100,
        positions: vec!["0,0".to_string()],
        names: vec![WORLD_NAME.to_string()],
        destinations_mode: true,
        ..Default::default()
    };
    let mut ids: Vec<String> = places
        .find_list(&both)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec![PLACE_ID.to_string(), WORLD_ROW_ID.to_string()]);
    assert_eq!(places.count_list(&both).await.unwrap(), 2);

    let worlds_only = PlaceListFilters {
        limit: 100,
        ids: vec![PLACE_ID.to_string(), WORLD_ROW_ID.to_string()],
        only_worlds: true,
        destinations_mode: true,
        ..Default::default()
    };
    let ids: Vec<String> = places
        .find_list(&worlds_only)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(
        ids,
        vec![WORLD_ROW_ID.to_string()],
        "kinds narrows an ids lookup"
    );

    scratch.drop().await;
}
