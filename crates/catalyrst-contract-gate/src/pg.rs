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
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}_{}_{}_{}", prefix, std::process::id(), nanos, seq)
}

pub struct ScratchDb {
    pub pool: PgPool,
    pub database: String,
    pub schemas: Vec<String>,
    admin_url: String,
}

impl ScratchDb {
    pub async fn create(env_var: &str, prefix: &str) -> Option<Self> {
        ScratchDbBuilder::new(env_var, prefix).build().await
    }

    pub async fn create_or_default(env_var: &str, default_url: &str, prefix: &str) -> Option<Self> {
        ScratchDbBuilder::new(env_var, prefix)
            .default_url(default_url)
            .build()
            .await
    }

    pub fn builder<'a>(env_var: &'a str, prefix: &'a str) -> ScratchDbBuilder<'a> {
        ScratchDbBuilder::new(env_var, prefix)
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

pub struct ScratchDbBuilder<'a> {
    env_var: &'a str,
    default_url: Option<&'a str>,
    prefix: &'a str,
    schemas: Vec<String>,
}

impl<'a> ScratchDbBuilder<'a> {
    pub fn new(env_var: &'a str, prefix: &'a str) -> Self {
        Self {
            env_var,
            default_url: None,
            prefix,
            schemas: Vec::new(),
        }
    }

    pub fn default_url(mut self, url: &'a str) -> Self {
        self.default_url = Some(url);
        self
    }

    pub fn schemas<I, S>(mut self, schemas: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.schemas = schemas.into_iter().map(Into::into).collect();
        self
    }

    pub async fn build(self) -> Option<ScratchDb> {
        let admin_url = admin_url(self.env_var, self.default_url)?;
        let admin = admin_pool(self.env_var, &admin_url).await?;
        let database = unique_suffix(self.prefix);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {}", database)))
            .execute(&admin)
            .await
            .unwrap_or_else(|e| panic!("CREATE DATABASE {database} failed: {e}"));
        let (base, _) = admin_url
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{} is not a postgres URL: {admin_url}", self.env_var));
        let db_url = if self.schemas.is_empty() {
            format!("{}/{}", base, database)
        } else {
            // public always trails the search_path: it stays reachable for
            // extensions/casts even though the named schemas take priority.
            let path = format!("{},public", self.schemas.join(","));
            format!("{}/{}?options=-c%20search_path%3D{}", base, database, path)
        };
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&db_url)
            .await
            .unwrap_or_else(|e| panic!("connect to scratch database {database} failed: {e}"));
        for schema in &self.schemas {
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("CREATE SCHEMA {schema} failed: {e}"));
        }
        Some(ScratchDb {
            pool,
            database,
            schemas: self.schemas,
            admin_url,
        })
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

#[cfg(test)]
mod scratch_db_schema_tests {
    use super::ScratchDb;

    const PG_VAR: &str = "CATALYRST_CONTRACT_GATE_TEST_PG";

    #[tokio::test]
    async fn a_named_schema_list_lands_on_the_search_path_ahead_of_public() {
        let Some(scratch) = ScratchDb::builder(PG_VAR, "cg_pg_schema")
            .schemas(["scratch_a", "scratch_b"])
            .build()
            .await
        else {
            return;
        };

        assert_eq!(scratch.schemas, vec!["scratch_a", "scratch_b"]);

        let path: String = sqlx::query_scalar("SHOW search_path")
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
        assert_eq!(path, "scratch_a,scratch_b,public");

        sqlx::query("CREATE TABLE marker (id int)")
            .execute(&scratch.pool)
            .await
            .unwrap();
        let owning_schema: String = sqlx::query_scalar(
            "SELECT table_schema FROM information_schema.tables WHERE table_name = 'marker'",
        )
        .fetch_one(&scratch.pool)
        .await
        .unwrap();
        assert_eq!(owning_schema, "scratch_a");

        scratch.drop().await;
    }

    #[tokio::test]
    async fn no_schemas_requested_behaves_like_plain_create() {
        let Some(plain) = ScratchDb::create(PG_VAR, "cg_pg_plain").await else {
            return;
        };
        let Some(built) = ScratchDb::builder(PG_VAR, "cg_pg_plain").build().await else {
            plain.drop().await;
            return;
        };

        assert!(plain.schemas.is_empty());
        assert!(built.schemas.is_empty());

        plain.drop().await;
        built.drop().await;
    }
}
