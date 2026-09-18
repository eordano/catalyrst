//! The limit-gated insert, the owner-aware delete, the visibility update and the
//! paged reads each collapsed into one statement; these pin the outcomes the
//! multi-statement versions produced. DB-gated via `CATALYRST_CAMERA_REEL_TEST_PG`
//! / `CATALYRST_TEST_PG`.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

use catalyrst_camera_reel::dto::{Image, Metadata};
use catalyrst_camera_reel::ports::db::Database;

const PG_VAR: &str = "CATALYRST_CAMERA_REEL_TEST_PG";
const A: &str = "0x1111111111111111111111111111111111111111";
const B: &str = "0x2222222222222222222222222222222222222222";

struct Scratch {
    db: Database,
    pool: PgPool,
    admin_url: String,
    schema: String,
}

async fn setup() -> Option<Scratch> {
    let url = catalyrst_testgate::require_pg(PG_VAR)?;
    let admin = match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            return catalyrst_testgate::pg_unusable(
                PG_VAR,
                &format!("connect to {url} failed: {e}"),
            )
        }
    };
    let schema = format!("test_cr_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&format!("{url}?options=-c%20search_path%3D{schema}"))
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/20260609000000_camera_reel_images.sql"),
        include_str!("../migrations/20260619000000_camera_reel_review_status.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    Some(Scratch {
        db: Database::new(pool.clone()),
        pool,
        admin_url: url,
        schema,
    })
}

impl Scratch {
    async fn done(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP SCHEMA {} CASCADE",
                self.schema
            )))
            .execute(&admin)
            .await;
        }
    }

    async fn is_public(&self, id: &str) -> Option<bool> {
        sqlx::query_scalar("SELECT is_public FROM camera_reel_images WHERE id = $1")
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_optional(&self.pool)
            .await
            .unwrap()
    }

    async fn count(&self, owner: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM camera_reel_images WHERE user_address = $1")
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

fn image(owner: &str, hash: &str, thumb: &str, is_public: bool, place_id: &str) -> Image {
    Image {
        id: Uuid::new_v4().to_string(),
        url: format!("http://127.0.0.1:5149/api/images/{hash}"),
        thumbnail_url: format!("http://127.0.0.1:5149/api/images/{thumb}"),
        is_public,
        metadata: Metadata {
            user_name: "owner".to_string(),
            user_address: owner.to_string(),
            date_time: "1700000000".to_string(),
            place_id: place_id.to_string(),
            ..Metadata::default()
        },
    }
}

#[tokio::test]
async fn the_insert_itself_enforces_the_per_user_limit() {
    let Some(s) = setup().await else { return };
    let first = image(A, "h1", "t1", true, "");
    assert_eq!(
        s.db.insert_image_within_limit(&first, 2).await.unwrap(),
        Some(1)
    );
    assert_eq!(
        s.db.insert_image_within_limit(&image(A, "h2", "t2", true, ""), 2)
            .await
            .unwrap(),
        Some(2)
    );
    assert_eq!(
        s.db.insert_image_within_limit(&image(A, "h3", "t3", true, ""), 2)
            .await
            .unwrap(),
        None,
        "the third row must be refused by the statement"
    );
    assert_eq!(s.count(A).await, 2);
    assert_eq!(
        s.db.insert_image_within_limit(&image(B, "h3", "t3", true, ""), 2)
            .await
            .unwrap(),
        Some(1),
        "limits are per user"
    );
    s.done().await;
}

#[tokio::test]
async fn owner_delete_reports_ownership_shared_blobs_and_remaining() {
    let Some(s) = setup().await else { return };
    let a1 = image(A, "h1", "t1", true, "");
    let a2 = image(A, "h1", "t2", true, "");
    let b1 = image(B, "h3", "t1", true, "");
    for img in [&a1, &a2, &b1] {
        s.db.insert_image(img).await.unwrap();
    }

    assert!(s
        .db
        .delete_image_owned(&Uuid::new_v4().to_string(), Some(A))
        .await
        .unwrap()
        .is_none());
    assert!(s
        .db
        .delete_image_owned("not-a-uuid", Some(A))
        .await
        .unwrap()
        .is_none());

    let refused =
        s.db.delete_image_owned(&a2.id, Some(B))
            .await
            .unwrap()
            .expect("row exists");
    assert!(
        !refused.deleted,
        "another user's image is reported, not deleted"
    );
    assert_eq!(s.count(A).await, 2);

    let d =
        s.db.delete_image_owned(&a1.id, Some(&A.to_uppercase()))
            .await
            .unwrap()
            .unwrap();
    assert!(d.deleted);
    assert_eq!(d.user_address, A);
    assert_eq!(
        (d.image_shared, d.thumbnail_shared, d.remaining),
        (true, true, 1)
    );

    let d =
        s.db.delete_image_owned(&a2.id, Some(A))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        (d.deleted, d.image_shared, d.thumbnail_shared, d.remaining),
        (true, false, false, 0)
    );

    let d =
        s.db.delete_image_owned(&b1.id, None)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        (d.deleted, d.image_shared, d.thumbnail_shared, d.remaining),
        (true, false, false, 0)
    );
    assert_eq!(s.count(A).await + s.count(B).await, 0);
    s.done().await;
}

#[tokio::test]
async fn visibility_update_keeps_404_403_and_noop_apart() {
    let Some(s) = setup().await else { return };
    let a1 = image(A, "h1", "t1", false, "");
    s.db.insert_image(&a1).await.unwrap();

    assert_eq!(
        s.db.set_image_visibility(&Uuid::new_v4().to_string(), A, true)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        s.db.set_image_visibility("nope", A, true).await.unwrap(),
        None
    );
    assert_eq!(
        s.db.set_image_visibility(&a1.id, B, true).await.unwrap(),
        Some(false)
    );
    assert_eq!(
        s.is_public(&a1.id).await,
        Some(false),
        "a stranger's update changes nothing"
    );
    assert_eq!(
        s.db.set_image_visibility(&a1.id, &A.to_uppercase(), true)
            .await
            .unwrap(),
        Some(true)
    );
    assert_eq!(s.is_public(&a1.id).await, Some(true));
    assert_eq!(
        s.db.set_image_visibility(&a1.id, A, true).await.unwrap(),
        Some(true),
        "no-op is still owned"
    );
    assert_eq!(s.is_public(&a1.id).await, Some(true));
    s.done().await;
}

#[tokio::test]
async fn pages_carry_the_total_and_fall_back_past_the_end() {
    let Some(s) = setup().await else { return };
    let place = Uuid::new_v4().to_string();
    for (i, public) in [true, true, true, false].into_iter().enumerate() {
        let place_id = if i < 2 { place.as_str() } else { "" };
        s.db.insert_image(&image(
            A,
            &format!("h{i}"),
            &format!("t{i}"),
            public,
            place_id,
        ))
        .await
        .unwrap();
    }

    let (rows, total) = s.db.get_user_images_page(A, 0, 2, true).await.unwrap();
    assert_eq!((rows.len(), total), (2, 3));
    let (rows, total) =
        s.db.get_user_images_page(&A.to_uppercase(), 0, 2, false)
            .await
            .unwrap();
    assert_eq!((rows.len(), total), (2, 4));
    let (rows, total) = s.db.get_user_images_page(A, 10, 2, true).await.unwrap();
    assert_eq!(
        (rows.len(), total),
        (0, 3),
        "past the end still reports the total"
    );
    let (rows, total) = s.db.get_user_images_page(A, 0, 0, false).await.unwrap();
    assert_eq!((rows.len(), total), (0, 4));
    let (rows, total) = s.db.get_user_images_page(B, 0, 5, false).await.unwrap();
    assert_eq!((rows.len(), total), (0, 0));

    let (rows, total) = s.db.get_place_images_page(&place, 0, 10).await.unwrap();
    assert_eq!((rows.len(), total), (2, 2));
    let (rows, total) = s.db.get_place_images_page(&place, 1, 10).await.unwrap();
    assert_eq!((rows.len(), total), (1, 2));
    let (rows, total) =
        s.db.get_multiple_places_images_page(std::slice::from_ref(&place), 0, 1)
            .await
            .unwrap();
    assert_eq!((rows.len(), total), (1, 2));
    assert!(s
        .db
        .get_place_images_page("not-a-uuid", 0, 10)
        .await
        .is_err());
    s.done().await;
}

#[tokio::test]
async fn servable_memo_is_dropped_by_review_and_delete() {
    let Some(s) = setup().await else { return };
    let a1 = image(A, "hx", "tx", true, "");
    s.db.insert_image(&a1).await.unwrap();

    assert!(s.db.hash_is_servable("hx").await.unwrap());
    assert!(s.db.hash_is_servable("tx").await.unwrap());
    assert_eq!(
        s.db.update_image_review_status(&a1.id, "rejected")
            .await
            .unwrap(),
        1
    );
    assert!(
        !s.db.hash_is_servable("hx").await.unwrap(),
        "rejection must not be masked by the memo"
    );
    assert!(!s.db.hash_is_servable("tx").await.unwrap());
    assert_eq!(
        s.db.update_image_review_status(&a1.id, "ok").await.unwrap(),
        1
    );
    assert!(s.db.hash_is_servable("hx").await.unwrap());
    assert_eq!(
        s.db.update_image_review_status(&Uuid::new_v4().to_string(), "ok")
            .await
            .unwrap(),
        0
    );

    assert_eq!(
        s.db.update_image_review_status(&a1.id, "rejected")
            .await
            .unwrap(),
        1
    );
    assert!(!s.db.hash_is_servable("hx").await.unwrap());
    let d =
        s.db.delete_image_owned(&a1.id, None)
            .await
            .unwrap()
            .unwrap();
    assert!(d.deleted);
    assert!(
        s.db.hash_is_servable("hx").await.unwrap(),
        "no referencing row: servable again"
    );
    s.done().await;
}
