use catalyrst_contract_gate::pg::ScratchSchema;
use sqlx::PgPool;

use catalyrst_places::ports::places::{PlaceListFilters, PlaceRow, PlacesComponent};

const WORLD_ID: &str = "world:hostile.dcl.eth";
const HOSTILE: &str =
    r#"Join <link="javascript:alert(1)">now</link> and <link="https://decentraland.org">us</link>"#;
const SANITIZED: &str = r#"Join now and <link="https://decentraland.org">us</link>"#;

const IMAGE_WORLD_ID: &str = "world:image.dcl.eth";
const HOSTILE_IMAGE: &str = r#"https://cdn.example/contents/x"><script>alert(1)</script>"#;
const ENCODED_IMAGE: &str = "https://cdn.example/contents/x%22%3E%3Cscript%3Ealert(1)%3C/script%3E";
const CURATED_HIGHLIGHT: &str = "/images/places/ice_poker_the_stronghold.jpeg";

const SCHEME_WORLD_ID: &str = "world:scheme.dcl.eth";
const CONTENT_IMAGE: &str = "https://peer.decentraland.org/content/contents/bafkreiabc";

const STUFFED_WORLD_ID: &str = "world:stuffed.dcl.eth";
const STUFFED: &str =
    r#"A quiet <color=#aquamarine>garden</color> <link="javascript:zeppelin">here</link>"#;
const STUFFED_PLAIN: &str = "A quiet garden here";

const HIDDEN_WORD: &str = "zeppelinword";
const SURVIVING_WORD: &str = "kryptonword";
const NESTED: [(&str, &str, &str); 4] = [
    ("nest:four", "Nest Four", "<<<<b>b>b>zeppelinword>"),
    ("nest:five", "Nest Five", "<<<<<b>b>b>b>zeppelinword>"),
    (
        "nest:link",
        "Nest Link",
        r#"<<<<b>b>b>link="javascript:alert(1)">"#,
    ),
    ("nest:deep", "Nest Deep", "<<<<<<b>b>b>b>b>kryptonword>"),
];

const QUOTED_IMAGE_ID: &str = "world:quoted.dcl.eth";
const QUOTED_IMAGE: &str = "https://cdn.example/a'.png";
const BACKTICK_IMAGE_ID: &str = "world:backtick.dcl.eth";
const BACKTICK_IMAGE: &str = "https://cdn.example/a`.png";

async fn setup(schema: &str) -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        schema,
    )
    .await
}

async fn search(places: &PlacesComponent, term: &str) -> Vec<PlaceRow> {
    places
        .find_list(&PlaceListFilters {
            limit: 10,
            search: Some(term.to_string()),
            ..Default::default()
        })
        .await
        .expect("find_list ok")
}

async fn seed_world(pool: &PgPool, id: &str, title: &str, description: &str) {
    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, title, description, raw) \
         VALUES ($1, '0,0', $2, $3, '{}'::jsonb)",
    )
    .bind(id)
    .bind(title)
    .bind(description)
    .execute(pool)
    .await
    .expect("seed world place");
}

async fn plain_of(pool: &PgPool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT description_plain FROM place_indexed WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("read description_plain")
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

#[tokio::test]
async fn world_descriptions_are_sanitized_on_every_read_path() {
    let Some(scratch) = setup("cg_places_description_sanitized").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, description, raw) VALUES ($1, $2, $3, $4)",
    )
    .bind(WORLD_ID)
    .bind("0,0")
    .bind(HOSTILE)
    .bind(serde_json::json!({ "world": true, "world_name": "hostile.dcl.eth" }))
    .execute(&pool)
    .await
    .expect("seed world place");

    let places = PlacesComponent::new(pool.clone());

    let by_id = places
        .find_by_id(WORLD_ID)
        .await
        .expect("find_by_id ok")
        .expect("world present");
    assert_eq!(
        by_id.description.as_deref(),
        Some(SANITIZED),
        "raw world metadata must not reach the places API"
    );

    let by_ids = places
        .find_by_ids(&[WORLD_ID.to_string()])
        .await
        .expect("find_by_ids ok");
    assert_eq!(by_ids[0].description.as_deref(), Some(SANITIZED));

    let listed = places
        .find_list(&PlaceListFilters {
            limit: 10,
            ids: vec![WORLD_ID.to_string()],
            ..Default::default()
        })
        .await
        .expect("find_list ok");
    assert_eq!(listed[0].description.as_deref(), Some(SANITIZED));

    scratch.drop().await;
}

#[tokio::test]
async fn search_matches_the_text_a_client_renders_not_the_stripped_markup() {
    let Some(scratch) = setup("cg_places_search_stripped").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, title, description, raw) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(STUFFED_WORLD_ID)
    .bind("0,0")
    .bind("Quiet Garden")
    .bind(STUFFED)
    .bind(serde_json::json!({ "world": true, "world_name": "stuffed.dcl.eth" }))
    .execute(&pool)
    .await
    .expect("seed world place");

    for (id, title, description) in NESTED {
        seed_world(&pool, id, title, description).await;
    }

    let places = PlacesComponent::new(pool.clone());

    let visible = search(&places, "garden").await;
    assert_eq!(
        visible.len(),
        1,
        "a word the client renders must stay findable"
    );
    assert_eq!(visible[0].description.as_deref(), Some(STUFFED_PLAIN));

    assert!(
        search(&places, "zeppelin").await.is_empty(),
        "a word that only exists inside a stripped link target must not be findable"
    );
    assert!(
        search(&places, "aquamarine").await.is_empty(),
        "a word that only exists inside a stripped markup tag must not be findable"
    );

    assert_eq!(
        search(&places, "nest").await.len(),
        NESTED.len(),
        "the nesting vectors must be visible to the search path at all"
    );
    for term in [HIDDEN_WORD, "javascript", "alert"] {
        assert!(
            search(&places, term).await.is_empty(),
            "{term} survives inside nested markup no client renders"
        );
    }

    let deep = search(&places, SURVIVING_WORD).await;
    assert_eq!(
        deep.len(),
        1,
        "a word the fail-closed strip leaves as bare text stays findable"
    );
    assert_eq!(
        deep[0].description.as_deref(),
        Some(SURVIVING_WORD),
        "search matches it because the client renders exactly that word"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn relevance_ranks_the_text_a_client_renders_not_the_markup() {
    let Some(scratch) = setup("cg_places_search_rank").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    let hidden = r#"<link="javascript:garden">g</link> "#.repeat(6) + "garden";
    seed_world(&pool, "rank:hidden", "Alpha Hall", &hidden).await;
    seed_world(&pool, "rank:visible", "Beta Hall", "garden garden garden").await;

    let places = PlacesComponent::new(pool.clone());
    let ranked = search(&places, "garden").await;
    let order: Vec<&str> = ranked.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        order,
        vec!["rank:visible", "rank:hidden"],
        "keywords repeated inside stripped markup must not buy ordering"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn the_archive_persists_a_plain_description_and_a_gated_image_url() {
    let Some(scratch) = setup("cg_places_plain_columns").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    sqlx::raw_sql(include_str!("../migrations/0004_place_plain_text.sql"))
        .execute(&pool)
        .await
        .expect("add the plain-text columns");

    sqlx::query(
        "INSERT INTO place_world_local (id, base_position, description, raw) VALUES ($1, $2, $3, $4)",
    )
    .bind(STUFFED_WORLD_ID)
    .bind("0,0")
    .bind(STUFFED)
    .bind(serde_json::json!({
        "world": true,
        "world_name": "stuffed.dcl.eth",
        "image": HOSTILE_IMAGE,
    }))
    .execute(&pool)
    .await
    .expect("seed world place");

    sqlx::query("INSERT INTO place (id, base_position, description, raw) VALUES ($1, $2, $3, $4)")
        .bind("place:genesis")
        .bind("0,0")
        .bind(HOSTILE)
        .bind(serde_json::json!({ "image": CONTENT_IMAGE }))
        .execute(&pool)
        .await
        .expect("seed place");

    let (plain, image): (String, Option<String>) =
        sqlx::query_as("SELECT description_plain, image_url FROM place_indexed WHERE id = $1")
            .bind(STUFFED_WORLD_ID)
            .fetch_one(&pool)
            .await
            .expect("read the world row through the view");
    assert_eq!(plain, STUFFED_PLAIN);
    assert_eq!(
        image, None,
        "an image carrying HTML-breakout characters must not be published at rest"
    );

    let (plain, image): (String, Option<String>) = sqlx::query_as(
        "SELECT description_plain, image_url FROM place_indexed WHERE id = 'place:genesis'",
    )
    .fetch_one(&pool)
    .await
    .expect("read the mirror row through the view");
    assert_eq!(
        plain, "Join now and us",
        "the plain column drops even the safe links the API preserves"
    );
    assert_eq!(image.as_deref(), Some(CONTENT_IMAGE));

    for (id, title, description) in NESTED {
        seed_world(&pool, id, title, description).await;
    }
    for (id, image) in [
        (QUOTED_IMAGE_ID, QUOTED_IMAGE),
        (BACKTICK_IMAGE_ID, BACKTICK_IMAGE),
    ] {
        sqlx::query(
            "INSERT INTO place_world_local (id, base_position, raw) VALUES ($1, '0,0', $2)",
        )
        .bind(id)
        .bind(serde_json::json!({ "image": image }))
        .execute(&pool)
        .await
        .expect("seed image world");
    }

    for (id, _, _) in NESTED.iter().take(3) {
        assert_eq!(
            plain_of(&pool, id).await.as_deref(),
            Some(""),
            "{id} must persist nothing a client would not render"
        );
    }
    assert_eq!(
        plain_of(&pool, NESTED[3].0).await.as_deref(),
        Some(SURVIVING_WORD),
        "past the pass cap the column keeps the bare text the sanitizer also keeps"
    );

    for id in [QUOTED_IMAGE_ID, BACKTICK_IMAGE_ID] {
        let image: Option<String> =
            sqlx::query_scalar("SELECT image_url FROM place_indexed WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("read image_url");
        assert_eq!(
            image, None,
            "{id} carries an attribute-breakout quote and must not be published at rest"
        );
    }

    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM place_indexed \
         WHERE description_plain ~ '</?[a-zA-Z][^>]*>' OR description_plain ~ '[<>]'",
    )
    .fetch_one(&pool)
    .await
    .expect("count rows with surviving markup");
    assert_eq!(
        leaked, 0,
        "no stored plain description may carry a tag or an angle bracket"
    );

    sqlx::raw_sql(include_str!("../migrations/0003_place_world_name.sql"))
        .execute(&pool)
        .await
        .expect("re-running an earlier migration must not break the view");

    let still_there: i64 =
        sqlx::query_scalar("SELECT count(*) FROM place_indexed WHERE description_plain <> ''")
            .fetch_one(&pool)
            .await
            .expect("view still exposes the plain column");
    assert_eq!(still_there, 3);

    scratch.drop().await;
}

#[tokio::test]
async fn recreating_the_world_table_keeps_the_plain_columns_generated() {
    let Some(scratch) = setup("cg_places_regenerate_columns").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../migrations/0004_place_plain_text.sql"))
            .execute(&pool)
            .await
            .expect("apply the plain-text columns");
        sqlx::query("DROP TABLE place_world_local CASCADE")
            .execute(&pool)
            .await
            .expect("drop the world table");
        sqlx::raw_sql(include_str!("../migrations/0002_place_indexed.sql"))
            .execute(&pool)
            .await
            .expect("recreate the world table");
        sqlx::raw_sql(include_str!("../migrations/0003_place_world_name.sql"))
            .execute(&pool)
            .await
            .expect("re-promote world_name");
    }

    sqlx::raw_sql(include_str!("../migrations/0004_place_plain_text.sql"))
        .execute(&pool)
        .await
        .expect("re-apply the plain-text columns");

    seed_world(&pool, STUFFED_WORLD_ID, "Quiet Garden", STUFFED).await;
    assert_eq!(
        plain_of(&pool, STUFFED_WORLD_ID).await.as_deref(),
        Some(STUFFED_PLAIN),
        "a recreated world table must not carry ungenerated plain columns"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn images_are_gated_on_read_while_curated_highlights_are_left_alone() {
    let Some(scratch) = setup("cg_places_image_sanitized").await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_place_table(&pool).await;

    for (id, name, image) in [
        (IMAGE_WORLD_ID, "image.dcl.eth", HOSTILE_IMAGE),
        (SCHEME_WORLD_ID, "scheme.dcl.eth", "javascript:alert(1)"),
    ] {
        sqlx::query("INSERT INTO place_world_local (id, base_position, raw) VALUES ($1, $2, $3)")
            .bind(id)
            .bind("0,0")
            .bind(serde_json::json!({
                "world": true,
                "world_name": name,
                "image": image,
                "highlighted_image": CURATED_HIGHLIGHT,
            }))
            .execute(&pool)
            .await
            .expect("seed world place");
    }

    sqlx::query("INSERT INTO place (id, base_position, raw) VALUES ($1, $2, $3)")
        .bind("place:genesis")
        .bind("0,0")
        .bind(serde_json::json!({ "image": CONTENT_IMAGE }))
        .execute(&pool)
        .await
        .expect("seed place");

    let places = PlacesComponent::new(pool.clone());

    let hostile = places
        .find_by_id(IMAGE_WORLD_ID)
        .await
        .expect("find_by_id ok")
        .expect("world present");
    assert_eq!(
        hostile.image.as_deref(),
        Some(ENCODED_IMAGE),
        "a stored image must never reach the API with raw markup characters"
    );
    assert_eq!(
        hostile.highlighted_image.as_deref(),
        Some(CURATED_HIGHLIGHT),
        "curated relative highlight images must survive the read path"
    );

    let scheme = places
        .find_by_id(SCHEME_WORLD_ID)
        .await
        .expect("find_by_id ok")
        .expect("world present");
    assert_eq!(scheme.image, None);

    let listed = places
        .find_list(&PlaceListFilters {
            limit: 10,
            ids: vec!["place:genesis".to_string()],
            ..Default::default()
        })
        .await
        .expect("find_list ok");
    assert_eq!(
        listed[0].image.as_deref(),
        Some(CONTENT_IMAGE),
        "content-server thumbnails must round-trip byte-identical"
    );

    scratch.drop().await;
}
