use sqlx::postgres::PgPool;

use crate::http::errors::ApiError;

#[derive(Clone)]
pub struct ListsComponent {
    pool: PgPool,
}

impl ListsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ready(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.pool).await.is_ok()
    }

    pub async fn pois(&self) -> Result<Vec<String>, ApiError> {
        self.column("SELECT coord FROM lists_poi ORDER BY coord")
            .await
    }

    pub async fn banned_names(&self) -> Result<Vec<String>, ApiError> {
        self.column("SELECT name FROM lists_banned_name ORDER BY name")
            .await
    }

    async fn column(&self, query: &str) -> Result<Vec<String>, ApiError> {
        match sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pool)
            .await
        {
            Ok(rows) => Ok(rows.into_iter().map(|(v,)| v).collect()),
            Err(sqlx::Error::Database(db)) if db.code().as_deref() == Some("42P01") => {
                tracing::warn!("list table not yet seeded; serving empty list");
                Ok(Vec::new())
            }
            Err(e) => Err(e.into()),
        }
    }
}
