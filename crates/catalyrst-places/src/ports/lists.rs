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

    pub async fn ensure_schema(&self) -> Result<(), ApiError> {
        for statement in split_statements(include_str!("../../migrations/0001_lists.sql")) {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&self.pool)
                .await?;
        }
        Ok(())
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

fn split_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    for line in sql.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        current.push_str(line);
        current.push('\n');
        if trimmed.ends_with(';') {
            statements.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        statements.push(current);
    }
    statements
}

#[cfg(test)]
mod tests {
    use super::split_statements;

    #[test]
    fn migration_splits_into_two_create_tables() {
        let stmts = split_statements(include_str!("../../migrations/0001_lists.sql"));
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("lists_poi"));
        assert!(stmts[1].contains("lists_banned_name"));
        assert!(stmts
            .iter()
            .all(|s| s.contains("CREATE TABLE IF NOT EXISTS")));
    }
}
