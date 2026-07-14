use std::str::FromStr;
use std::sync::Arc;

use axum::Router;
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_contract_gate::{Case, Gate};
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use catalyrst_places::clients::{CommsGatekeeper, Events, Presence};
use catalyrst_places::ports::lists::ListsComponent;
use catalyrst_places::ports::places::{
    PlaceListFilters, PlacesComponent, PLACEHOLDER_TITLES, PLACEHOLDER_TITLE_SUFFIX_REGEX,
    TEST_WORD_TITLE_REGEX,
};
use catalyrst_places::{api_router_with_spec, AppState, AppStateInner};

const REAL_IMAGE: &str = "https://example.com/real-thumbnail.png";
const MAP_FALLBACK_IMAGE: &str =
    "https://api.decentraland.org/v2/map.png?height=1024&width=1024&selected=1%2C1";
const WORLD_DEFAULT_THUMBNAIL_IMAGE: &str = "https://worlds-content-server.decentraland.org/contents/bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y";
const UNWANTED_THUMBNAIL_IMAGE: &str = "https://peer.decentraland.org/content/contents/bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";
const ROAD_PARCEL: &str = "-1,-10";
const OWNER: &str = "0x4f7fe261619141ffa63fefee35bba886581292f4";
const DEPLOYER: &str = "0x0000000000000000000000000000000000000abc";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_contentgate",
    )
    .await
}

async fn create_schema(pool: &PgPool) {
    for migration in [
        include_str!("../migrations/0000_place.sql"),
        include_str!("../migrations/0002_place_indexed.sql"),
        include_str!("../migrations/0003_place_world_name.sql"),
        include_str!("../migrations/0005_road_positions.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(pool)
            .await
            .expect("apply migration");
    }
}

struct Fixture {
    id: &'static str,
    title: Option<&'static str>,
    image: Option<&'static str>,
    owner: Option<&'static str>,
    creator_address: Option<&'static str>,
    contact_name: Option<&'static str>,
    base_position: &'static str,
    highlighted: bool,
    world_name: Option<&'static str>,
}

fn place(id: &'static str, title: &'static str) -> Fixture {
    Fixture {
        id,
        title: Some(title),
        image: Some(REAL_IMAGE),
        owner: Some(OWNER),
        creator_address: Some(OWNER),
        contact_name: Some("METATIGER"),
        base_position: "100,100",
        highlighted: false,
        world_name: None,
    }
}

// Shaped like the deployment's world-places sync writes a local world:
// the deployer is the only attribution it can carry, and only once the
// worlds index reports one.
fn world(id: &'static str, name: &'static str, title: &'static str) -> Fixture {
    Fixture {
        id,
        title: Some(title),
        image: Some(REAL_IMAGE),
        owner: None,
        creator_address: Some(DEPLOYER),
        contact_name: None,
        base_position: "0,0",
        highlighted: false,
        world_name: Some(name),
    }
}

async fn seed(pool: &PgPool, f: &Fixture) {
    let mut raw = serde_json::json!({
        "image": f.image,
        "owner": f.owner,
        "contact_name": f.contact_name,
        "positions": [f.base_position],
    });
    if let Some(name) = f.world_name {
        raw["world"] = serde_json::json!(true);
        raw["world_name"] = serde_json::json!(name);
        raw["creator_address"] = serde_json::json!(f.creator_address);
        sqlx::query(
            "INSERT INTO place_world_local \
             (id, base_position, title, creator_address, highlighted, raw, world, world_name) \
             VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7)",
        )
        .bind(f.id)
        .bind(f.base_position)
        .bind(f.title)
        .bind(f.creator_address)
        .bind(f.highlighted)
        .bind(raw)
        .bind(name)
        .execute(pool)
        .await
        .expect("seed world");
        return;
    }
    sqlx::query(
        "INSERT INTO place (id, base_position, title, creator_address, highlighted, raw) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(f.id)
    .bind(f.base_position)
    .bind(f.title)
    .bind(f.creator_address)
    .bind(f.highlighted)
    .bind(raw)
    .execute(pool)
    .await
    .expect("seed place");
}

fn feed() -> PlaceListFilters {
    PlaceListFilters {
        limit: 100,
        order_desc: true,
        destinations_mode: true,
        require_content_places: true,
        require_content_worlds: true,
        ..Default::default()
    }
}

async fn ids(places: &PlacesComponent, filters: &PlaceListFilters) -> Vec<String> {
    let mut out: Vec<String> = places
        .find_list(filters)
        .await
        .expect("destinations list")
        .into_iter()
        .map(|r| r.id)
        .collect();
    out.sort();
    out
}

fn sorted_ids(fixtures: &[Fixture]) -> Vec<String> {
    let mut out: Vec<String> = fixtures.iter().map(|f| f.id.to_string()).collect();
    out.sort();
    out
}

async fn assert_feed(places: &PlacesComponent, filters: &PlaceListFilters, expected: &[Fixture]) {
    let expected = sorted_ids(expected);
    assert_eq!(ids(places, filters).await, expected);
    assert_eq!(
        places.count_list(filters).await.expect("count"),
        expected.len() as i64,
        "the total must count only what the listing shows"
    );
}

#[tokio::test]
async fn generic_feed_hides_fallback_images_placeholder_titles_and_unclaimed_places() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_schema(&pool).await;

    let kept_places = [
        place("amber", "Amber Hollow"),
        Fixture {
            owner: None,
            creator_address: None,
            contact_name: Some("METATIGER"),
            ..place("halcyon", "Halcyon Works")
        },
        place("land5", "The Land 5"),
        place("level3", "Level 3"),
        place("contest", "contest"),
        place("contest-arena", "Contest Arena"),
        place("latest", "Latest News"),
        place("testing", "Testing Grounds"),
        place("marble", "Marble Observatory"),
        Fixture {
            title: Some("Untitled"),
            image: Some(MAP_FALLBACK_IMAGE),
            highlighted: true,
            ..place("curated", "")
        },
    ];
    let hidden_places = [
        Fixture {
            image: Some(MAP_FALLBACK_IMAGE),
            ..place("basalt", "Basalt Terrace")
        },
        Fixture {
            owner: None,
            creator_address: None,
            contact_name: None,
            ..place("verdant", "Verdant Annex")
        },
        Fixture {
            owner: None,
            creator_address: Some(OWNER),
            contact_name: None,
            ..place("deployer-only", "Obsidian Yard")
        },
        Fixture {
            owner: None,
            creator_address: None,
            contact_name: Some("SDK"),
            ..place("slate", "Slate Works")
        },
        Fixture {
            base_position: ROAD_PARCEL,
            contact_name: Some("Decentraland Foundation"),
            ..place("kerbside", "Kerbside Gallery")
        },
        Fixture {
            image: None,
            ..place("cinder", "Cinder Wharf")
        },
        Fixture {
            image: Some(""),
            ..place("hollow", "Hollow Quay")
        },
        Fixture {
            image: Some("   "),
            ..place("pale", "Pale Cistern")
        },
        Fixture {
            image: Some(WORLD_DEFAULT_THUMBNAIL_IMAGE),
            ..place("driftwood", "Driftwood Pier")
        },
        Fixture {
            image: Some(UNWANTED_THUMBNAIL_IMAGE),
            ..place("ember", "Ember Courtyard")
        },
        place("streaming", "streaming_test"),
        place("newscene6", "New Scene 6"),
        place("scene5", "Scene 5"),
        place("contest-camel", "conTest"),
        place("abtest", "ABTestScene1"),
        place("testplaza", "Test Plaza"),
        place("untitled", "Untitled"),
        place("untitled-space", "Untitled "),
        place("empty-title", ""),
        place("blank-title", "   "),
        Fixture {
            title: None,
            ..place("no-title", "")
        },
        place("thetestscene", "TheTestScene"),
    ];
    let kept_worlds = [
        world("w-deployed", "amberhollow.dcl.eth", "Amber Hollow World"),
        Fixture {
            creator_address: None,
            contact_name: Some("Amber Hollow Studio"),
            ..world("w-contact", "studio.dcl.eth", "Studio Hall")
        },
        Fixture {
            creator_address: None,
            contact_name: None,
            ..world("w-unclaimed", "unclaimed.dcl.eth", "Quiet Harbour")
        },
        Fixture {
            base_position: ROAD_PARCEL,
            ..world("w-on-road", "roadside.dcl.eth", "Roadside Lodge")
        },
        Fixture {
            title: Some("Untitled"),
            image: Some(MAP_FALLBACK_IMAGE),
            highlighted: true,
            ..world("w-curated", "curated.dcl.eth", "")
        },
    ];
    let hidden_worlds = [
        Fixture {
            image: Some(WORLD_DEFAULT_THUMBNAIL_IMAGE),
            ..world(
                "w-default-thumb",
                "defaultthumb.dcl.eth",
                "Basalt Terrace World",
            )
        },
        Fixture {
            image: None,
            ..world("w-no-image", "noimage.dcl.eth", "Cinder Wharf World")
        },
        world("w-untitled", "untitled.dcl.eth", "Untitled 2"),
        world("w-test", "testscene.dcl.eth", "TestScene"),
    ];
    for f in kept_places
        .iter()
        .chain(hidden_places.iter())
        .chain(kept_worlds.iter())
        .chain(hidden_worlds.iter())
    {
        seed(&pool, f).await;
    }

    let places = PlacesComponent::new(pool.clone());
    assert!(
        places.probe_road_positions().await.expect("probe"),
        "0005 must have created road_positions"
    );

    let mut kept: Vec<Fixture> = Vec::new();
    kept.extend(kept_places);
    kept.extend(kept_worlds);
    assert_feed(&places, &feed(), &kept).await;

    let (kept_places, kept_worlds): (Vec<Fixture>, Vec<Fixture>) =
        kept.into_iter().partition(|f| f.world_name.is_none());
    assert_feed(
        &places,
        &PlaceListFilters {
            only_worlds: true,
            ..feed()
        },
        &kept_worlds,
    )
    .await;
    assert_feed(
        &places,
        &PlaceListFilters {
            only_places: true,
            ..feed()
        },
        &kept_places,
    )
    .await;

    let by_pointer = PlaceListFilters {
        positions: vec![ROAD_PARCEL.to_string()],
        only_places: true,
        require_content_places: false,
        ..feed()
    };
    assert_eq!(
        ids(&places, &by_pointer).await,
        vec!["kerbside".to_string()],
        "a road place asked for by pointer is still answered"
    );

    let by_name = PlaceListFilters {
        names: vec!["noimage.dcl.eth".to_string()],
        require_content_worlds: false,
        ..feed()
    };
    assert_eq!(
        ids(&places, &by_name).await,
        vec!["w-no-image".to_string()],
        "a world asked for by name is still answered"
    );

    let ungated = PlaceListFilters {
        require_content_places: false,
        require_content_worlds: false,
        ..feed()
    };
    assert_eq!(
        ids(&places, &ungated).await.len(),
        kept_places.len() + hidden_places.len() + kept_worlds.len() + hidden_worlds.len(),
        "a lookup query sees every row"
    );

    scratch.drop().await;
}

async fn names_a_test_scene(pool: &PgPool, title: &str) -> bool {
    sqlx::query_scalar("SELECT $1::text ~ $2::text")
        .bind(title)
        .bind(TEST_WORD_TITLE_REGEX)
        .fetch_one(pool)
        .await
        .expect("test-word regex")
}

async fn is_placeholder_title(pool: &PgPool, title: &str) -> bool {
    let titles: Vec<String> = PLACEHOLDER_TITLES.iter().map(|t| t.to_string()).collect();
    sqlx::query_scalar(
        "SELECT LOWER(TRIM(REGEXP_REPLACE($1::text, $2::text, ''))) = ANY ($3::text[])",
    )
    .bind(title)
    .bind(PLACEHOLDER_TITLE_SUFFIX_REGEX)
    .bind(titles)
    .fetch_one(pool)
    .await
    .expect("placeholder title match")
}

// The unit tests in content_quality.rs run the same patterns through the
// `regex` crate; this is the verdict, under the dialect the feed queries use.
#[tokio::test]
async fn title_regexes_behave_the_same_under_postgres() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();

    for (title, caught) in [
        ("Test Plaza", true),
        ("streaming_test", true),
        ("test-scene", true),
        ("TEST", true),
        ("test", true),
        ("My test", true),
        ("conTest", true),
        ("TheTestScene", true),
        ("ABTestScene1", true),
        ("TestScene", true),
        ("A Test", true),
        ("contest", false),
        ("Contest", false),
        ("Contest Arena", false),
        ("Latest", false),
        ("Latest News", false),
        ("protest", false),
        ("testament", false),
        ("Testing Grounds", false),
        ("abtestscene1", false),
        ("Amber Hollow", false),
        ("Scene of the Crime", false),
        ("Angzaar User Shop #38", false),
    ] {
        assert_eq!(
            names_a_test_scene(&pool, title).await,
            caught,
            "{title:?} test-word verdict"
        );
    }

    for (title, caught) in [
        ("Untitled", true),
        ("Untitled ", true),
        ("Untitled 3", true),
        ("New Scene 6", true),
        ("Scene 5", true),
        ("Scene", true),
        ("scene", true),
        ("SDK7 Scene Template", true),
        ("Empty", true),
        ("empty scene 12", true),
        ("interactive-text", true),
        ("Interactive Text", true),
        ("The Land 5", false),
        ("Level 3", false),
        ("Scene of the Crime", false),
        ("Scenery", false),
        ("Amber Hollow", false),
        ("Angzaar User Shop #38", false),
    ] {
        assert_eq!(
            is_placeholder_title(&pool, title).await,
            caught,
            "{title:?} placeholder verdict"
        );
    }

    scratch.drop().await;
}

async fn app(pool: PgPool) -> (Router, Gate) {
    let places = PlacesComponent::new(pool.clone());
    places.probe_road_positions().await.expect("probe");
    let state: AppState = Arc::new(AppStateInner {
        places,
        lists: ListsComponent::new(pool),
        admin_addresses: vec![],
        data_team_auth_token: None,
        admin_auth_token: None,
        comms_gatekeeper: CommsGatekeeper::new("http://127.0.0.1:9".into()),
        events: Events::new("http://127.0.0.1:9".into()),
        presence: Presence::new("http://127.0.0.1:9".into()),
        gossip: Arc::new(catalyrst_fed::NoopPublisher),
        domain: catalyrst_fed::sig::domains::places(),
    });
    let (router, spec) = api_router_with_spec();
    let gate = Gate::new(serde_json::to_value(&spec).expect("spec"));
    (router.with_state(state), gate)
}

// Upstream answers `owner=` with `LOWER(p.owner) = $owner OR base_position IN
// (operated parcels)`; this mirror has no owner column to compare, so a wallet
// operating no parcel must get an empty page, never the feed.
#[tokio::test]
async fn an_unknown_owner_gets_an_empty_page_rather_than_the_feed() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_schema(&pool).await;
    seed(&pool, &place("amber", "Amber Hollow")).await;
    seed(
        &pool,
        &Fixture {
            image: Some(MAP_FALLBACK_IMAGE),
            ..place("basalt", "Basalt Terrace")
        },
    )
    .await;
    let (app, mut gate) = app(pool.clone()).await;

    let page = gate
        .hit(
            &app,
            Case::new("get", "/api/destinations").query("limit=10"),
        )
        .await;
    assert_eq!(page["total"], json!(1), "the bare feed is gated");
    assert_eq!(page["data"][0]["id"], json!("amber"));

    let owned = gate
        .hit(
            &app,
            Case::new("get", "/api/destinations")
                .query("owner=0x000000000000000000000000000000000000dead&limit=10"),
        )
        .await;
    assert_eq!(owned["total"], json!(0));
    assert_eq!(owned["data"], json!([]));

    let places = PlacesComponent::new(pool.clone());
    let filters = PlaceListFilters {
        owner_filtered: true,
        require_content_places: false,
        require_content_worlds: false,
        ..feed()
    };
    assert!(ids(&places, &filters).await.is_empty());
    assert_eq!(places.count_list(&filters).await.expect("count"), 0);

    scratch.drop().await;
}

const ROLE_PASSWORD: &str = "cg-places-role";

async fn create_role(
    admin: &PgPool,
    schema: &str,
    prefix: &str,
    schema_privileges: &str,
) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!("{prefix}_{}_{nanos}", std::process::id());
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE ROLE {name} LOGIN PASSWORD '{ROLE_PASSWORD}'"
    )))
    .execute(admin)
    .await
    .unwrap_or_else(|e| {
        panic!("CREATE ROLE {name} failed: {e}; this test needs a CATALYRST_PLACES_TEST_PG user allowed to create roles")
    });
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT {schema_privileges} ON SCHEMA {schema} TO {name}"
    )))
    .execute(admin)
    .await
    .expect("grant schema privileges");
    name
}

async fn role_pool(scratch: &ScratchSchema, role: &str) -> PgPool {
    let options = PgConnectOptions::from_str(&scratch.url())
        .expect("scratch url")
        .username(role)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap_or_else(|e| panic!("connect as {role} failed: {e}"))
}

async fn drop_roles(admin: &PgPool, roles: &[&str]) {
    for role in roles {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {role} CASCADE")))
            .execute(admin)
            .await
            .expect("drop owned");
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
            .execute(admin)
            .await
            .expect("drop role");
    }
}

// The deployment's layout: the archive owner (writer) owns `place`, bootstrap
// defines place_indexed as the superuser and grants the reader from there,
// so the writer is neither grantor nor grantee of that SELECT and
// information_schema shows it nothing. 0005 then lands through the writer
// at startup, and the reader must still end up able to read the table; while
// it cannot, the probe has to say so and the feed has to keep answering.
#[tokio::test]
async fn road_grant_reaches_the_reader_and_the_probe_asks_for_select_not_existence() {
    let Some(scratch) = setup().await else {
        return;
    };
    let admin = scratch.pool.clone();
    let writer = create_role(&admin, &scratch.schema, "cg_places_writer", "CREATE, USAGE").await;
    let reader = create_role(&admin, &scratch.schema, "cg_places_reader", "USAGE").await;
    let writer_pool = role_pool(&scratch, &writer).await;
    let reader_pool = role_pool(&scratch, &reader).await;

    sqlx::raw_sql(include_str!("../migrations/0000_place.sql"))
        .execute(&writer_pool)
        .await
        .expect("the writer owns place");
    for migration in [
        include_str!("../migrations/0002_place_indexed.sql"),
        include_str!("../migrations/0003_place_world_name.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&admin)
            .await
            .expect("bootstrap defines the view as the superuser");
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT SELECT ON place, place_world_local, place_indexed TO {reader}"
    )))
    .execute(&admin)
    .await
    .expect("reader grant");
    seed(&admin, &place("amber", "Amber Hollow")).await;
    seed(
        &admin,
        &Fixture {
            base_position: ROAD_PARCEL,
            contact_name: Some("Decentraland Foundation"),
            ..place("kerbside", "Kerbside Gallery")
        },
    )
    .await;
    let with_road = vec!["amber".to_string(), "kerbside".to_string()];
    let without_road = vec!["amber".to_string()];

    let places = PlacesComponent::new(reader_pool.clone());
    assert!(
        !places.probe_road_positions().await.expect("probe"),
        "no table yet"
    );
    assert_eq!(
        ids(&places, &feed()).await,
        with_road,
        "roads stay in until the table lands"
    );

    sqlx::raw_sql(include_str!("../migrations/0005_road_positions.sql"))
        .execute(&writer_pool)
        .await
        .expect("0005 applies as the non-superuser writer");
    let reader_can_select: bool =
        sqlx::query_scalar("SELECT has_table_privilege($1, 'road_positions', 'SELECT')")
            .bind(&reader)
            .fetch_one(&admin)
            .await
            .expect("privilege check");
    assert!(
        reader_can_select,
        "0005's grant loop must find the reader in pg_class.relacl; information_schema hides a superuser's grant from the writer"
    );
    assert!(places.probe_road_positions().await.expect("probe"));
    assert_eq!(
        ids(&places, &feed()).await,
        without_road,
        "the road parcel is hidden once the reader can consult the table"
    );

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "REVOKE SELECT ON road_positions FROM {reader}"
    )))
    .execute(&admin)
    .await
    .expect("revoke");
    assert!(
        !places.probe_road_positions().await.expect("probe"),
        "the table exists but the reader cannot read it: existence alone must not pass"
    );
    assert_eq!(
        ids(&places, &feed()).await,
        with_road,
        "without SELECT the road leg is dropped and the feed still answers"
    );
    assert_eq!(places.count_list(&feed()).await.expect("count"), 2);

    reader_pool.close().await;
    writer_pool.close().await;
    drop_roles(&admin, &[&writer, &reader]).await;
    scratch.drop().await;
}
