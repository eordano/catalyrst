use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn admin_url(env_var: &str, fallback: Option<&str>) -> Option<String> {
    match fallback {
        Some(f) => Some(catalyrst_testgate::require_pg_or(env_var, f)),
        None => catalyrst_testgate::require_pg(env_var),
    }
}

async fn admin_pool(env_var: &str, url: &str) -> Option<PgPool> {
    match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
    {
        Ok(pool) => Some(pool),
        Err(e) => {
            catalyrst_testgate::pg_unusable(env_var, &format!("connect to {url} failed: {e}"))
        }
    }
}

fn unique_suffix(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}_{}_{}", prefix, std::process::id(), nanos)
}

pub struct ScratchDb {
    pub pool: PgPool,
    pub database: String,
    admin_url: String,
}

impl ScratchDb {
    pub async fn create(env_var: &str, prefix: &str) -> Option<Self> {
        Self::create_at(env_var, None, prefix).await
    }

    pub async fn create_or_default(env_var: &str, default_url: &str, prefix: &str) -> Option<Self> {
        Self::create_at(env_var, Some(default_url), prefix).await
    }

    async fn create_at(env_var: &str, default_url: Option<&str>, prefix: &str) -> Option<Self> {
        let admin_url = admin_url(env_var, default_url)?;
        let admin = admin_pool(env_var, &admin_url).await?;
        let database = unique_suffix(prefix);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {}", database)))
            .execute(&admin)
            .await
            .unwrap_or_else(|e| panic!("CREATE DATABASE {database} failed: {e}"));
        let (base, _) = admin_url
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{env_var} is not a postgres URL: {admin_url}"));
        let db_url = format!("{}/{}", base, database);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&db_url)
            .await
            .unwrap_or_else(|e| panic!("connect to scratch database {database} failed: {e}"));
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
        Self::create_at(env_var, None, prefix).await
    }

    pub async fn create_or_default(env_var: &str, default_url: &str, prefix: &str) -> Option<Self> {
        Self::create_at(env_var, Some(default_url), prefix).await
    }

    async fn create_at(env_var: &str, default_url: Option<&str>, prefix: &str) -> Option<Self> {
        let url = admin_url(env_var, default_url)?;
        let admin = admin_pool(env_var, &url).await?;
        let schema = unique_suffix(prefix);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
            .execute(&admin)
            .await
            .unwrap_or_else(|e| panic!("CREATE SCHEMA {schema} failed: {e}"));
        let suffixed = format!("{}?options=-c%20search_path%3D{}", url, schema);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&suffixed)
            .await
            .unwrap_or_else(|e| panic!("connect to scratch schema {schema} failed: {e}"));
        Some(Self {
            pool,
            schema,
            admin_url: url,
        })
    }

    pub fn url(&self) -> String {
        format!(
            "{}?options=-c%20search_path%3D{}",
            self.admin_url, self.schema
        )
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
