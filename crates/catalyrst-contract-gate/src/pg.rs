use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub struct ScratchDb {
    pub pool: PgPool,
    pub database: String,
    admin_url: String,
}

impl ScratchDb {
    pub async fn create(env_var: &str, prefix: &str) -> Option<Self> {
        let admin_url = std::env::var(env_var).ok()?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&admin_url)
            .await
            .ok()?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let database = format!("{}_{}_{}", prefix, std::process::id(), nanos);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {}", database)))
            .execute(&admin)
            .await
            .ok()?;
        let (base, _) = admin_url.rsplit_once('/')?;
        let db_url = format!("{}/{}", base, database);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&db_url)
            .await
            .ok()?;
        Some(Self {
            pool,
            database,
            admin_url,
        })
    }

    pub async fn apply_sql(&self, sql: &str) {
        apply_statements(&self.pool, sql).await;
    }

    pub async fn drop(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&self.admin_url)
            .await
        {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE {} WITH (FORCE)",
                self.database
            )))
            .execute(&admin)
            .await;
        }
    }
}

async fn apply_statements(pool: &PgPool, sql: &str) {
    let mut statement = String::new();
    for line in sql.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        statement.push_str(line);
        statement.push('\n');
        if trimmed.ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("migration stmt failed: {e}\n{statement}"));
            statement.clear();
        }
    }
    if !statement.trim().is_empty() {
        sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("trailing migration stmt failed: {e}\n{statement}"));
    }
}

pub struct ScratchSchema {
    pub pool: PgPool,
    pub schema: String,
    admin_url: String,
}

impl ScratchSchema {
    pub async fn create(env_var: &str, prefix: &str) -> Option<Self> {
        let url = std::env::var(env_var).ok()?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&url)
            .await
            .ok()?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let schema = format!("{}_{}_{}", prefix, std::process::id(), nanos);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
            .execute(&admin)
            .await
            .ok()?;
        let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&suffixed)
            .await
            .ok()?;
        Some(Self {
            pool,
            schema,
            admin_url: url,
        })
    }

    pub async fn apply_sql(&self, sql: &str) {
        apply_statements(&self.pool, sql).await;
    }

    pub async fn drop(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
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
}
