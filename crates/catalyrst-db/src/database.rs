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

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5433,
            database: "content".into(),
            user: "postgres".into(),
            password: String::new(),
            max_connections: 10,
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
        ) -> Pin<Box<dyn Future<Output = Result<T, DatabaseError>> + Send + 'c>>,
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
