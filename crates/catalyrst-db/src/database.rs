use std::future::Future;
use std::pin::Pin;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::{Executor, Postgres, Transaction};
use thiserror::Error;
use tracing::{error, info};

pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub max_connections: u32,
    pub idle_timeout_secs: u64,
    pub query_timeout_secs: u64,
}

pub const DEFAULT_PG_POOL_SIZE: u32 = 20;

pub fn parse_pg_pool_size() -> u32 {
    parse_pg_pool_size_from(std::env::var("PG_POOL_SIZE").ok().as_deref())
}

fn parse_pg_pool_size_from(raw: Option<&str>) -> u32 {
    match raw {
        Some(raw) => match raw.trim().parse::<i64>() {
            Ok(parsed) => parsed.max(1) as u32,
            Err(_) => DEFAULT_PG_POOL_SIZE,
        },
        None => DEFAULT_PG_POOL_SIZE,
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5433,
            database: "content".into(),
            user: "postgres".into(),
            password: String::new(),
            max_connections: parse_pg_pool_size(),
            idle_timeout_secs: 30,
            query_timeout_secs: 60,
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("connection failed — did you run the migrations? {0}")]
    ConnectionFailed(sqlx::Error),
}

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(cfg: &DatabaseConfig) -> Result<Self, DatabaseError> {
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            cfg.user, cfg.password, cfg.host, cfg.port, cfg.database
        );

        let connect_opts: PgConnectOptions = url
            .parse::<PgConnectOptions>()
            .map_err(DatabaseError::ConnectionFailed)?
            .options([
                ("statement_timeout", "60000"),
                ("idle_in_transaction_session_timeout", "30000"),
            ]);

        let pool = PgPoolOptions::new()
            .max_connections(cfg.max_connections)
            .idle_timeout(std::time::Duration::from_secs(cfg.idle_timeout_secs))
            .connect_with(connect_opts)
            .await
            .map_err(DatabaseError::ConnectionFailed)?;

        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn verify(&self) -> Result<(), DatabaseError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(DatabaseError::ConnectionFailed)?;
        conn.execute("SELECT 1")
            .await
            .map_err(DatabaseError::ConnectionFailed)?;
        Ok(())
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, DatabaseError> {
        Ok(self.pool.begin().await?)
    }

    pub async fn transaction<F, T>(&self, f: F) -> Result<T, DatabaseError>
    where
        F: for<'c> FnOnce(
            &'c mut Transaction<'_, Postgres>,
        )
            -> Pin<Box<dyn Future<Output = Result<T, DatabaseError>> + Send + 'c>>,
        T: Send,
    {
        let mut tx = self.pool.begin().await?;
        let val = f(&mut tx).await.map_err(|e| {
            error!("Transaction failed: {e}");
            e
        })?;
        tx.commit().await?;
        Ok(val)
    }

    pub async fn close(&self) {
        info!("Draining database connections");
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_pool_size_defaults_when_unset() {
        assert_eq!(parse_pg_pool_size_from(None), DEFAULT_PG_POOL_SIZE);
        assert_eq!(DEFAULT_PG_POOL_SIZE, 20);
    }

    #[test]
    fn pg_pool_size_defaults_when_non_numeric() {
        assert_eq!(parse_pg_pool_size_from(Some("abc")), DEFAULT_PG_POOL_SIZE);
        assert_eq!(parse_pg_pool_size_from(Some("")), DEFAULT_PG_POOL_SIZE);
        assert_eq!(parse_pg_pool_size_from(Some("12x")), DEFAULT_PG_POOL_SIZE);
    }

    #[test]
    fn pg_pool_size_honors_override() {
        assert_eq!(parse_pg_pool_size_from(Some("50")), 50);
        assert_eq!(parse_pg_pool_size_from(Some("1000")), 1000);
        assert_eq!(parse_pg_pool_size_from(Some("  30  ")), 30);
    }

    #[test]
    fn pg_pool_size_floors_at_one() {
        assert_eq!(parse_pg_pool_size_from(Some("0")), 1);
        assert_eq!(parse_pg_pool_size_from(Some("-5")), 1);
    }
}
