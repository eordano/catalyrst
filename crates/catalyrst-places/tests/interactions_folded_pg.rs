use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_places::ports::places::{PlaceListFilters, PlaceOrderBy, PlacesComponent};
use sqlx::PgPool;

const A: &str = "123e4567-e89b-12d3-a456-4266141740a0";
const B: &str = "123e4567-e89b-12d3-a456-4266141740b0";
const C: &str = "123e4567-e89b-12d3-a456-4266141740c0";
const USER: &str = "0x000000000000000000000000000000000000BEEF";
const OTHER: &str = "0x000000000000000000000000000000000000cafe";

async fn setup() -> Option<ScratchSchema> {
    ScratchSchema::create_or_default(
        "CATALYRST_PLACES_TEST_PG",
        "postgres://postgres:postgres@127.0.0.1:5432/places",
        "cg_places_fold",
    )
    .await
}

async fn create_catalog(pool: &PgPool) {
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
    for (id, pos, score) in [(A, "0,0", 3.0), (B, "1,1", 2.0), (C, "2,2", 1.0)] {
        sqlx::query(
            "INSERT INTO place (id, title, base_position, deployed_at, raw) \
             VALUES ($1, $2, $3, now(), $4)",
        )
        .bind(id)
        .bind(format!("place {id}"))
        .bind(pos)
        .bind(serde_json::json!({ "positions": [pos], "world": false, "like_score": score }))
        .execute(pool)
        .await
        .expect("seed place");
    }
}

async fn component(pool: &PgPool) -> PlacesComponent {
    let places = PlacesComponent::new(pool.clone()).with_writer(pool.clone());
    places.ensure_local_schema().await.unwrap();
    places
}

fn list(order_by: PlaceOrderBy, limit: i64, offset: i64) -> PlaceListFilters {
    PlaceListFilters {
        limit,
        offset,
        order_by,
        order_desc: true,
        only_places: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn the_probe_folds_only_when_the_reader_is_the_writers_database() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_catalog(&pool).await;

    let reader_only = PlacesComponent::new(pool.clone());
    assert!(!reader_only.probe_interactions().await.unwrap());
    assert!(!reader_only.viewer_in_query());

    let places = component(&pool).await;
    assert!(places.probe_interactions().await.unwrap());
    assert!(places.viewer_in_query());

    scratch.drop().await;
}

#[tokio::test]
async fn a_page_carries_its_total_and_the_viewers_interactions_in_one_statement() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_catalog(&pool).await;
    let places = component(&pool).await;
    places.set_favorite(A, USER, true, 0, false).await.unwrap();
    places
        .set_like(B, USER, Some(true), 0.0, 0, 0, false, false)
        .await
        .unwrap();
    places
        .set_like(C, USER, Some(false), 0.0, 0, 0, false, false)
        .await
        .unwrap();
    places.set_favorite(C, OTHER, true, 0, false).await.unwrap();

    for fold in [false, true] {
        if fold {
            assert!(places.probe_interactions().await.unwrap());
        }
        for order in [PlaceOrderBy::LikeScore, PlaceOrderBy::UpdatedAt] {
            let mut f = list(order, 2, 0);
            assert!(places
                .scope_to_viewer(&mut f, Some(USER), false)
                .await
                .unwrap());
            let (rows, total) = places.list_page(&f).await.unwrap();
            assert_eq!(
                total, 3,
                "fold={fold} order={order:?}: total is the full match count"
            );
            assert_eq!(total, places.count_list(&f).await.unwrap());
            assert_eq!(rows.len(), 2);
            let by_id = |id: &str| rows.iter().find(|r| r.id == id);
            if let Some(a) = by_id(A) {
                assert!(a.user_favorite && !a.user_like && !a.user_dislike);
            }
            if let Some(b) = by_id(B) {
                assert!(!b.user_favorite && b.user_like && !b.user_dislike);
            }
            if let Some(c) = by_id(C) {
                assert!(!c.user_favorite && !c.user_like && c.user_dislike);
            }

            let (past, total) = places.list_page(&list(order, 2, 10)).await.unwrap();
            assert!(past.is_empty());
            assert_eq!(
                total, 3,
                "an empty page past the offset still reports the total"
            );

            let mut favs = list(order, 10, 0);
            assert!(places
                .scope_to_viewer(&mut favs, Some(USER), true)
                .await
                .unwrap());
            let (rows, total) = places.list_page(&favs).await.unwrap();
            assert_eq!(
                rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
                vec![A]
            );
            assert_eq!(total, 1);
            assert!(rows[0].user_favorite);

            let mut none = list(order, 10, 0);
            assert!(!places.scope_to_viewer(&mut none, None, true).await.unwrap());
            let mut stranger = list(order, 10, 0);
            let scoped = places
                .scope_to_viewer(
                    &mut stranger,
                    Some("0x0000000000000000000000000000000000000001"),
                    true,
                )
                .await
                .unwrap();
            if scoped {
                let (rows, total) = places.list_page(&stranger).await.unwrap();
                assert!(rows.is_empty());
                assert_eq!(total, 0);
            }
        }

        let one = places.find_by_id_for(B, Some(USER)).await.unwrap().unwrap();
        assert!(one.user_like && !one.user_favorite);
        let anon = places.find_by_id_for(B, None).await.unwrap().unwrap();
        assert!(!anon.user_like);
        let many = places
            .find_by_ids_for(&[A.to_string(), C.to_string()], Some(USER))
            .await
            .unwrap();
        assert_eq!(many.len(), 2);
        assert!(many.iter().any(|r| r.id == A && r.user_favorite));
        assert!(many
            .iter()
            .any(|r| r.id == C && r.user_dislike && !r.user_favorite));
    }

    scratch.drop().await;
}

#[tokio::test]
async fn interaction_writes_keep_their_counts_in_one_statement() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    create_catalog(&pool).await;
    let places = component(&pool).await;

    assert_eq!(
        places.set_favorite(A, USER, true, 0, false).await.unwrap(),
        (1, true)
    );
    assert_eq!(
        places.set_favorite(A, USER, true, 0, false).await.unwrap(),
        (1, true)
    );
    assert_eq!(
        places.set_favorite(A, OTHER, true, 0, false).await.unwrap(),
        (2, true)
    );
    let stored: i32 = sqlx::query_scalar("SELECT favorites FROM place WHERE id = $1")
        .bind(A)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 2);
    assert_eq!(
        places.set_favorite(A, USER, false, 0, false).await.unwrap(),
        (1, false)
    );
    assert_eq!(
        places.set_favorite(A, USER, false, 0, false).await.unwrap(),
        (1, false)
    );

    let like = |u: &'static str, v: Option<bool>, act: f64| {
        let places = &places;
        async move {
            places
                .set_like(B, u, v, act, 0, 0, false, false)
                .await
                .unwrap()
        }
    };
    assert_eq!(like(USER, Some(true), 150.0).await, (1, 0, true, false));
    assert_eq!(like(OTHER, Some(false), 10.0).await, (1, 1, false, true));
    assert_eq!(like(USER, Some(false), 150.0).await, (0, 2, false, true));
    let (likes, dislikes, rate, score): (i32, i32, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT likes, dislikes, (raw->>'like_rate')::float8, (raw->>'like_score')::float8 \
         FROM place WHERE id = $1",
    )
    .bind(B)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((likes, dislikes), (0, 2));
    assert_eq!(
        rate,
        Some(0.0),
        "only the active voter counts toward the rate"
    );
    assert!(score.is_some());
    assert_eq!(like(USER, None, 0.0).await, (0, 1, false, false));
    assert_eq!(like(OTHER, None, 0.0).await, (0, 0, false, false));

    // An entity without a catalog row still answers its counts.
    let orphan = places
        .set_like("no-such-row", USER, Some(true), 0.0, 0, 0, false, false)
        .await
        .unwrap();
    assert_eq!(orphan, (1, 0, true, false));
    assert_eq!(
        places
            .set_favorite("no-such-row", USER, true, 0, false)
            .await
            .unwrap(),
        (1, true)
    );

    scratch.drop().await;
}
