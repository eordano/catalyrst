use sqlx::types::{chrono, Json, Uuid};
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::dto::{Image, Metadata};

#[derive(sqlx::FromRow, Debug)]
pub struct DbImage {
    pub id: Uuid,
    pub user_address: String,
    pub url: String,
    pub thumbnail_url: String,
    pub is_public: bool,
    #[allow(dead_code)]
    pub created_at: chrono::NaiveDateTime,
    pub metadata: Json<Metadata>,
    pub review_status: String,
}

pub const TABLE: &str = "camera_reel_images";

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

fn parse_uuid(uuid: &str) -> Result<Uuid, sqlx::Error> {
    Uuid::parse_str(uuid).map_err(|_| sqlx::Error::Protocol("Invalid UUID".to_string()))
}

enum Filter<'a> {
    UserAddress(&'a str),
    PlaceId(&'a str),
    PlacesIds(&'a [String]),
}

impl Database {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_image(&self, id: &str) -> Result<DbImage, sqlx::Error> {
        let sql = format!("SELECT * FROM {TABLE} WHERE id = $1");
        sqlx::query_as::<_, DbImage>(sqlx::AssertSqlSafe(sql))
            .bind(parse_uuid(id)?)
            .fetch_one(&self.pool)
            .await
    }

    fn build_query(
        select: &str,
        filter: &Filter<'_>,
        public_only: bool,
    ) -> Result<QueryBuilder<Postgres>, sqlx::Error> {
        let mut qb = QueryBuilder::new(select);
        qb.push(" WHERE ");
        match filter {
            Filter::UserAddress(addr) => {
                qb.push("user_address = ");
                qb.push_bind(addr.to_lowercase());
            }
            Filter::PlaceId(place_id) => {
                qb.push("metadata->>'placeId' = ");
                let uuid = parse_uuid(place_id)?.to_string();
                qb.push_bind(uuid);
            }
            Filter::PlacesIds(ids) => {
                qb.push("metadata->>'placeId' = ANY(");
                qb.push_bind(*ids);
                qb.push(")");
            }
        }
        if public_only {
            // Non-owner / public reads must never surface moderator-rejected
            // images. Owner-authenticated reads (public_only = false) keep the
            // existing "owner can see their own" visibility model.
            qb.push(" AND is_public = true AND review_status <> 'rejected'");
        }
        Ok(qb)
    }

    async fn get_images(
        &self,
        filter: Filter<'_>,
        offset: i64,
        limit: i64,
        public_only: bool,
    ) -> Result<Vec<DbImage>, sqlx::Error> {
        let select = format!("SELECT * FROM {TABLE}");
        let mut qb = Self::build_query(&select, &filter, public_only)?;
        qb.push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        qb.build_query_as::<DbImage>().fetch_all(&self.pool).await
    }

    async fn get_images_count(
        &self,
        filter: Filter<'_>,
        public_only: bool,
    ) -> Result<u64, sqlx::Error> {
        let select = format!("SELECT COUNT(*) FROM {TABLE}");
        let mut qb = Self::build_query(&select, &filter, public_only)?;
        let count = qb.build_query_scalar::<i64>().fetch_one(&self.pool).await?;
        Ok(count as u64)
    }

    pub async fn get_user_images(
        &self,
        user: &str,
        offset: i64,
        limit: i64,
        public_only: bool,
    ) -> Result<Vec<DbImage>, sqlx::Error> {
        self.get_images(Filter::UserAddress(user), offset, limit, public_only)
            .await
    }

    pub async fn get_user_images_count(
        &self,
        user: &str,
        public_only: bool,
    ) -> Result<u64, sqlx::Error> {
        self.get_images_count(Filter::UserAddress(user), public_only)
            .await
    }

    pub async fn get_place_images(
        &self,
        place_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<DbImage>, sqlx::Error> {
        self.get_images(Filter::PlaceId(place_id), offset, limit, true)
            .await
    }

    pub async fn get_place_images_count(&self, place_id: &str) -> Result<u64, sqlx::Error> {
        self.get_images_count(Filter::PlaceId(place_id), true).await
    }

    pub async fn get_multiple_places_images(
        &self,
        places_ids: &[String],
        offset: i64,
        limit: i64,
    ) -> Result<Vec<DbImage>, sqlx::Error> {
        self.get_images(Filter::PlacesIds(places_ids), offset, limit, true)
            .await
    }

    pub async fn get_multiple_places_images_count(
        &self,
        places_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        self.get_images_count(Filter::PlacesIds(places_ids), true)
            .await
    }

    pub async fn insert_image(&self, image: &Image) -> Result<(), sqlx::Error> {
        let sql = format!(
            "INSERT INTO {TABLE} (id, user_address, url, thumbnail_url, is_public, metadata) \
             VALUES ($1, $2, $3, $4, $5, $6)"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(parse_uuid(&image.id)?)
            .bind(image.metadata.user_address.to_lowercase())
            .bind(&image.url)
            .bind(&image.thumbnail_url)
            .bind(image.is_public)
            .bind(Json(&image.metadata))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_image(&self, id: &str) -> Result<(), sqlx::Error> {
        let sql = format!("DELETE FROM {TABLE} WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(parse_uuid(id)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Count rows other than `exclude_id` whose `url` or `thumbnail_url` points
    /// at the content-addressed `hash` (last path segment `/{hash}`). Used to
    /// keep a shared blob alive while any other row still references it, since
    /// blobs are keyed purely by content hash and multiple rows can share one.
    pub async fn count_other_images_with_hash(
        &self,
        hash: &str,
        exclude_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let sql = format!(
            "SELECT COUNT(*) FROM {TABLE} \
             WHERE id <> $1 AND (url LIKE $2 OR thumbnail_url LIKE $2)"
        );
        let pattern = format!("%/{hash}");
        let count = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(parse_uuid(exclude_id)?)
            .bind(pattern)
            .fetch_one(&self.pool)
            .await?;
        Ok(count as u64)
    }

    /// Whether the content-addressed `hash` may still be served as bytes.
    /// Blobs are keyed purely by content hash and can be shared by several
    /// rows, so a hash is withheld only when every row referencing it has been
    /// moderator-`rejected`. Hashes with no referencing row at all (e.g. legacy
    /// or orphaned blobs) remain servable -- rejection is the sole gate here.
    pub async fn hash_is_servable(&self, hash: &str) -> Result<bool, sqlx::Error> {
        let sql = format!(
            "SELECT \
               EXISTS(SELECT 1 FROM {TABLE} \
                      WHERE (url LIKE $1 OR thumbnail_url LIKE $1) \
                        AND review_status <> 'rejected') \
               OR NOT EXISTS(SELECT 1 FROM {TABLE} \
                      WHERE url LIKE $1 OR thumbnail_url LIKE $1)"
        );
        let pattern = format!("%/{hash}");
        let servable = sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(sql))
            .bind(pattern)
            .fetch_one(&self.pool)
            .await?;
        Ok(servable)
    }

    pub async fn update_image_visibility(
        &self,
        id: &str,
        is_public: bool,
    ) -> Result<(), sqlx::Error> {
        let sql = format!("UPDATE {TABLE} SET is_public = $1 WHERE id = $2");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(is_public)
            .bind(parse_uuid(id)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_image_review_status(
        &self,
        id: &str,
        review_status: &str,
    ) -> Result<u64, sqlx::Error> {
        let sql = format!("UPDATE {TABLE} SET review_status = $1 WHERE id = $2");
        let res = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(review_status)
            .bind(parse_uuid(id)?)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}
