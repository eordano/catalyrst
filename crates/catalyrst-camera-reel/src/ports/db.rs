use std::time::Duration;

use moka::future::Cache;
use sqlx::types::{chrono, Json, Uuid};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Row};

use crate::dto::{Image, Metadata};

const SERVABLE_TTL: Duration = Duration::from_secs(60);
const SERVABLE_MAX_ENTRIES: u64 = 10_000;

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

/// What one `DELETE` statement reports back: ownership outcome, whether each blob is
/// still referenced by another row, and the owner's remaining image count.
#[derive(Debug)]
pub struct DeletedImage {
    pub user_address: String,
    pub url: String,
    pub thumbnail_url: String,
    pub deleted: bool,
    pub image_shared: bool,
    pub thumbnail_shared: bool,
    pub remaining: u64,
}

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
    servable: Cache<String, bool>,
}

fn parse_uuid(uuid: &str) -> Result<Uuid, sqlx::Error> {
    Uuid::parse_str(uuid).map_err(|_| sqlx::Error::Protocol("Invalid UUID".to_string()))
}

pub fn hash_of_url(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

enum Filter<'a> {
    UserAddress(&'a str),
    PlaceId(&'a str),
    PlacesIds(&'a [String]),
}

impl Database {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            servable: Cache::builder()
                .max_capacity(SERVABLE_MAX_ENTRIES)
                .time_to_live(SERVABLE_TTL)
                .build(),
        }
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
        filter: &Filter<'_>,
        public_only: bool,
    ) -> Result<u64, sqlx::Error> {
        let select = format!("SELECT COUNT(*) FROM {TABLE}");
        let mut qb = Self::build_query(&select, filter, public_only)?;
        let count = qb.build_query_scalar::<i64>().fetch_one(&self.pool).await?;
        Ok(count as u64)
    }

    /// Page plus total in one statement; only a page past the end (or `limit` 0)
    /// falls back to a bare COUNT.
    async fn get_images_page(
        &self,
        filter: Filter<'_>,
        offset: i64,
        limit: i64,
        public_only: bool,
    ) -> Result<(Vec<DbImage>, u64), sqlx::Error> {
        let select = format!("SELECT *, COUNT(*) OVER () AS total FROM {TABLE}");
        let mut qb = Self::build_query(&select, &filter, public_only)?;
        qb.push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let rows = qb.build().fetch_all(&self.pool).await?;
        let Some(first) = rows.first() else {
            return Ok((
                Vec::new(),
                self.get_images_count(&filter, public_only).await?,
            ));
        };
        let total = first.try_get::<i64, _>("total")? as u64;
        let images = rows
            .iter()
            .map(DbImage::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((images, total))
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

    pub async fn get_user_images_page(
        &self,
        user: &str,
        offset: i64,
        limit: i64,
        public_only: bool,
    ) -> Result<(Vec<DbImage>, u64), sqlx::Error> {
        self.get_images_page(Filter::UserAddress(user), offset, limit, public_only)
            .await
    }

    pub async fn get_user_images_count(
        &self,
        user: &str,
        public_only: bool,
    ) -> Result<u64, sqlx::Error> {
        self.get_images_count(&Filter::UserAddress(user), public_only)
            .await
    }

    pub async fn get_place_images_page(
        &self,
        place_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<DbImage>, u64), sqlx::Error> {
        self.get_images_page(Filter::PlaceId(place_id), offset, limit, true)
            .await
    }

    pub async fn get_multiple_places_images_page(
        &self,
        places_ids: &[String],
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<DbImage>, u64), sqlx::Error> {
        self.get_images_page(Filter::PlacesIds(places_ids), offset, limit, true)
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

    /// Inserts only while the owner is under `max_images`; `None` means the limit
    /// held. The returned count already includes the new row.
    pub async fn insert_image_within_limit(
        &self,
        image: &Image,
        max_images: u64,
    ) -> Result<Option<u64>, sqlx::Error> {
        let sql = format!(
            "WITH ins AS ( \
                 INSERT INTO {TABLE} (id, user_address, url, thumbnail_url, is_public, metadata) \
                 SELECT $1, $2, $3, $4, $5, $6 \
                 WHERE (SELECT COUNT(*) FROM {TABLE} WHERE user_address = $2) < $7 \
                 RETURNING id \
             ) \
             SELECT (SELECT COUNT(*) FROM {TABLE} WHERE user_address = $2) + 1 FROM ins"
        );
        let count = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(parse_uuid(&image.id)?)
            .bind(image.metadata.user_address.to_lowercase())
            .bind(&image.url)
            .bind(&image.thumbnail_url)
            .bind(image.is_public)
            .bind(Json(&image.metadata))
            .bind(max_images.min(i64::MAX as u64) as i64)
            .fetch_optional(&self.pool)
            .await?;
        Ok(count.map(|c| c as u64))
    }

    pub async fn delete_image(&self, id: &str) -> Result<(), sqlx::Error> {
        let sql = format!("DELETE FROM {TABLE} WHERE id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(parse_uuid(id)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// One statement: deletes the row (only the owner's when `owner` is given) and
    /// reports, from the same snapshot, whether each blob hash is still referenced by
    /// another row and how many images the owner has left. `None` is "no such image";
    /// `deleted == false` is an ownership mismatch, with nothing removed.
    pub async fn delete_image_owned(
        &self,
        id: &str,
        owner: Option<&str>,
    ) -> Result<Option<DeletedImage>, sqlx::Error> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        let sql = format!(
            "WITH target AS ( \
                 SELECT id, user_address, url, thumbnail_url, \
                        '%/' || substring(url from '[^/]*$') AS image_pat, \
                        '%/' || substring(thumbnail_url from '[^/]*$') AS thumb_pat \
                 FROM {TABLE} WHERE id = $1 \
             ), del AS ( \
                 DELETE FROM {TABLE} \
                 WHERE id = $1 AND ($2::text IS NULL OR user_address = $2) \
                 RETURNING id \
             ) \
             SELECT t.user_address, t.url, t.thumbnail_url, \
                    EXISTS (SELECT 1 FROM del) AS deleted, \
                    EXISTS (SELECT 1 FROM {TABLE} o WHERE o.id <> t.id \
                            AND (o.url LIKE t.image_pat OR o.thumbnail_url LIKE t.image_pat)) \
                        AS image_shared, \
                    EXISTS (SELECT 1 FROM {TABLE} o WHERE o.id <> t.id \
                            AND (o.url LIKE t.thumb_pat OR o.thumbnail_url LIKE t.thumb_pat)) \
                        AS thumbnail_shared, \
                    (SELECT COUNT(*) FROM {TABLE} o WHERE o.user_address = t.user_address) - 1 \
                        AS remaining \
             FROM target t"
        );
        let row = sqlx::query_as::<_, (String, String, String, bool, bool, bool, i64)>(
            sqlx::AssertSqlSafe(sql),
        )
        .bind(id)
        .bind(owner.map(str::to_lowercase))
        .fetch_optional(&self.pool)
        .await?;
        let Some((
            user_address,
            url,
            thumbnail_url,
            deleted,
            image_shared,
            thumbnail_shared,
            remaining,
        )) = row
        else {
            return Ok(None);
        };
        if deleted {
            self.servable.invalidate(hash_of_url(&url)).await;
            self.servable.invalidate(hash_of_url(&thumbnail_url)).await;
        }
        Ok(Some(DeletedImage {
            user_address,
            url,
            thumbnail_url,
            deleted,
            image_shared,
            thumbnail_shared,
            remaining: remaining.max(0) as u64,
        }))
    }

    /// Matches `hash` as the last path segment of `url` or `thumbnail_url`. Blobs are keyed
    /// purely by content hash, so several rows can share one and it must outlive any single
    /// deletion.
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

    /// Withheld only when every row referencing the hash is moderator-`rejected`; a hash
    /// with no referencing row at all (legacy or orphaned blob) stays servable. Rejection is
    /// the sole gate. Memoised for `SERVABLE_TTL`; review and delete writes drop the entry.
    pub async fn hash_is_servable(&self, hash: &str) -> Result<bool, sqlx::Error> {
        if let Some(servable) = self.servable.get(hash).await {
            return Ok(servable);
        }
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
        self.servable.insert(hash.to_string(), servable).await;
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

    /// `None`: no such image. `Some(false)`: not the owner's, unchanged. `Some(true)`:
    /// owned, and now at `is_public` (a no-op when it already was).
    pub async fn set_image_visibility(
        &self,
        id: &str,
        owner: &str,
        is_public: bool,
    ) -> Result<Option<bool>, sqlx::Error> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        let sql = format!(
            "WITH target AS ( \
                 SELECT user_address FROM {TABLE} WHERE id = $1 \
             ), upd AS ( \
                 UPDATE {TABLE} SET is_public = $3 \
                 WHERE id = $1 AND user_address = $2 AND is_public <> $3 \
                 RETURNING id \
             ) \
             SELECT user_address = $2 FROM target"
        );
        sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(owner.to_lowercase())
            .bind(is_public)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn update_image_review_status(
        &self,
        id: &str,
        review_status: &str,
    ) -> Result<u64, sqlx::Error> {
        let sql = format!(
            "UPDATE {TABLE} SET review_status = $1 WHERE id = $2 RETURNING url, thumbnail_url"
        );
        let rows = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql))
            .bind(review_status)
            .bind(parse_uuid(id)?)
            .fetch_all(&self.pool)
            .await?;
        for (url, thumbnail_url) in &rows {
            self.servable.invalidate(hash_of_url(url)).await;
            self.servable.invalidate(hash_of_url(thumbnail_url)).await;
        }
        Ok(rows.len() as u64)
    }
}
