use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;

use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

pub const DEFAULT_LIST_NAME: &str = "Favorites";

#[derive(Debug, Serialize)]
pub struct FavoriteList {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "userAddress")]
    pub user_address: String,

    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(rename = "isPrivate")]
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(rename = "itemsCount")]
    pub items_count: i64,
    #[serde(rename = "previewOfItemIds")]
    pub preview_of_item_ids: Vec<String>,
}

pub struct ListsComponent {
    pool: PgPool,
    write: Option<PgPool>,
}

fn is_missing_favorites(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e {
        matches!(
            db.code().as_deref(),
            Some("42P01") | Some("42501") | Some("3F000")
        )
    } else {
        false
    }
}

pub fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

impl ListsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, write: None }
    }

    pub fn with_write(mut self, pool: PgPool) -> Self {
        self.write = Some(pool);
        self
    }

    fn write_pool(&self) -> &PgPool {
        self.write.as_ref().unwrap_or(&self.pool)
    }

    async fn notify_dirty(&self) {
        match sqlx::query("SELECT pg_notify('catalyrst_market_dirty', 'favorites')")
            .execute(self.write_pool())
            .await
        {
            Ok(_) => tracing::debug!("favorites dirty notify sent"),
            Err(err) => tracing::warn!(
                %err,
                "favorites dirty notify failed (stale reads bounded by cache TTL)"
            ),
        }
    }

    pub async fn item_exists(&self, item_id: &str) -> Result<bool, ApiError> {
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {schema}.item WHERE id = $1 OR (collection_id || '-' || blockchain_id::text) = $1) AS found",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(item_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get::<bool, _>("found").unwrap_or(false))
    }

    pub async fn count_owned_lists(
        &self,
        user_address: &str,
        list_ids: &[String],
    ) -> Result<usize, ApiError> {
        if list_ids.is_empty() {
            return Ok(0);
        }
        let row = sqlx::query(sqlx::AssertSqlSafe(
            "SELECT COUNT(*)::int8 AS total FROM favorites.lists \
             WHERE id = ANY($1::uuid[]) AND user_address = $2"
                .to_string(),
        ))
        .bind(list_ids)
        .bind(user_address.to_lowercase())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get::<i64, _>("total").unwrap_or(0) as usize)
    }

    pub async fn get_or_create_default_list(&self, user_address: &str) -> Result<String, ApiError> {
        let user = user_address.to_lowercase();
        let existing = sqlx::query(sqlx::AssertSqlSafe(
            "SELECT id::text AS id FROM favorites.lists \
             WHERE user_address = $1 AND name = $2 \
             ORDER BY created_at ASC LIMIT 1"
                .to_string(),
        ))
        .bind(&user)
        .bind(DEFAULT_LIST_NAME)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = existing {
            return Ok(row.try_get::<String, _>("id").unwrap_or_default());
        }
        let row = sqlx::query(sqlx::AssertSqlSafe(
            "INSERT INTO favorites.lists (name, user_address, is_private) \
             VALUES ($1, $2, true) RETURNING id::text AS id"
                .to_string(),
        ))
        .bind(DEFAULT_LIST_NAME)
        .bind(&user)
        .fetch_one(self.write_pool())
        .await?;
        Ok(row.try_get::<String, _>("id").unwrap_or_default())
    }

    pub async fn pick_in_lists(
        &self,
        item_id: &str,
        user_address: &str,
        list_ids: &[String],
    ) -> Result<(), ApiError> {
        if list_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(sqlx::AssertSqlSafe(
            "INSERT INTO favorites.picks (item_id, user_address, list_id) \
             SELECT $1, $2, id FROM favorites.lists \
             WHERE id = ANY($3::uuid[]) AND user_address = $2 \
             ON CONFLICT (item_id, list_id) DO NOTHING"
                .to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .bind(list_ids)
        .execute(self.write_pool())
        .await?;
        self.notify_dirty().await;
        Ok(())
    }

    pub async fn unpick_from_lists(
        &self,
        item_id: &str,
        user_address: &str,
        list_ids: &[String],
    ) -> Result<(), ApiError> {
        if list_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(sqlx::AssertSqlSafe(
            "DELETE FROM favorites.picks \
             WHERE item_id = $1 AND user_address = $2 AND list_id = ANY($3::uuid[])"
                .to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .bind(list_ids)
        .execute(self.write_pool())
        .await?;
        self.notify_dirty().await;
        Ok(())
    }

    pub async fn unpick_everywhere(
        &self,
        item_id: &str,
        user_address: &str,
    ) -> Result<u64, ApiError> {
        let res = sqlx::query(sqlx::AssertSqlSafe(
            "DELETE FROM favorites.picks WHERE item_id = $1 AND user_address = $2".to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .execute(self.write_pool())
        .await?;
        self.notify_dirty().await;
        Ok(res.rows_affected())
    }

    pub async fn is_picked_by_user(
        &self,
        item_id: &str,
        user_address: &str,
    ) -> Result<bool, ApiError> {
        let row = sqlx::query(sqlx::AssertSqlSafe(
            "SELECT EXISTS(SELECT 1 FROM favorites.picks \
             WHERE item_id = $1 AND user_address = $2) AS found"
                .to_string(),
        ))
        .bind(item_id)
        .bind(user_address.to_lowercase())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get::<bool, _>("found").unwrap_or(false))
    }

    pub async fn get_lists(
        &self,
        user_address: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<FavoriteList>, i64), ApiError> {
        let where_clause = if user_address.is_some() {
            "WHERE l.user_address = $1"
        } else {
            ""
        };
        let (limit_p, offset_p) = if user_address.is_some() {
            ("$2", "$3")
        } else {
            ("$1", "$2")
        };

        let sql = format!(
            r#"
SELECT
  l.id::text                                   AS id,
  l.name                                       AS name,
  l.description                                AS description,
  l.user_address                               AS user_address,
  (EXTRACT(EPOCH FROM l.created_at) * 1000)::int8 AS created_at,
  (EXTRACT(EPOCH FROM l.updated_at) * 1000)::int8 AS updated_at,
  l.is_private                                 AS is_private,
  l.permission                                 AS permission,
  COALESCE(pc.cnt, 0)::int8                    AS items_count,
  COALESCE(pp.preview, ARRAY[]::text[])        AS preview
FROM favorites.lists l
LEFT JOIN (
  SELECT list_id, COUNT(*) AS cnt FROM favorites.picks GROUP BY list_id
) pc ON pc.list_id = l.id
LEFT JOIN (
  SELECT list_id, ARRAY_AGG(item_id ORDER BY created_at DESC) AS preview
  FROM (
    SELECT list_id, item_id, created_at,
           ROW_NUMBER() OVER (PARTITION BY list_id ORDER BY created_at DESC) AS rn
    FROM favorites.picks
  ) ranked
  WHERE rn <= 4
  GROUP BY list_id
) pp ON pp.list_id = l.id
{where_clause}
ORDER BY l.created_at DESC, l.id ASC
LIMIT {limit_p} OFFSET {offset_p}
"#,
        );

        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        if let Some(addr) = user_address {
            q = q.bind(addr.to_lowercase());
        }
        q = q.bind(limit).bind(offset);

        let rows = match q.fetch_all(&self.pool).await {
            Ok(rows) => rows,
            Err(e) if is_missing_favorites(&e) => return Ok((Vec::new(), 0)),
            Err(e) => return Err(e.into()),
        };

        let lists: Vec<FavoriteList> = rows
            .iter()
            .map(|r| FavoriteList {
                id: r.try_get("id").unwrap_or_default(),
                name: r.try_get("name").unwrap_or_default(),
                description: r
                    .try_get::<Option<String>, _>("description")
                    .unwrap_or(None),
                user_address: r.try_get("user_address").unwrap_or_default(),
                created_at: r.try_get::<i64, _>("created_at").unwrap_or(0),
                updated_at: r.try_get::<Option<i64>, _>("updated_at").unwrap_or(None),
                is_private: r.try_get::<bool, _>("is_private").unwrap_or(false),
                permission: r.try_get::<Option<String>, _>("permission").unwrap_or(None),
                items_count: r.try_get::<i64, _>("items_count").unwrap_or(0),
                preview_of_item_ids: r.try_get::<Vec<String>, _>("preview").unwrap_or_default(),
            })
            .collect();

        let count_sql =
            format!("SELECT COUNT(*)::int8 AS total FROM favorites.lists l {where_clause}");
        let mut cq = sqlx::query(sqlx::AssertSqlSafe(count_sql));
        if let Some(addr) = user_address {
            cq = cq.bind(addr.to_lowercase());
        }
        let total = match cq.fetch_one(&self.pool).await {
            Ok(row) => row.try_get::<i64, _>("total").unwrap_or(0),
            Err(e) if is_missing_favorites(&e) => 0,
            Err(e) => return Err(e.into()),
        };

        Ok((lists, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_dirty_literal_matches_dirty_channel() {
        assert_eq!(
            crate::ports::catalog_cache::DIRTY_CHANNEL,
            "catalyrst_market_dirty"
        );
    }

    #[test]
    fn uuid_validator() {
        assert!(is_uuid("01337f44-b985-45be-a4f6-6a4efeb40412"));
        assert!(!is_uuid("01337f44b98545bea4f66a4efeb40412"));
        assert!(!is_uuid("zz337f44-b985-45be-a4f6-6a4efeb40412"));
    }
}
