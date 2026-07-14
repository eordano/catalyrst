use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use thiserror::Error;

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

    #[error("connection failed \u{2014} did you run the migrations? {0}")]
    ConnectionFailed(sqlx::Error),
}

/// Knobs for [`connect_pool`]. Defaults mirror the canonical service pool:
/// 10 connections, 30s idle timeout, no acquire timeout, 60s statement timeout.
#[derive(Debug, Clone)]
pub struct PoolSettings {
    pub max_connections: u32,
    pub idle_timeout_secs: u64,
    pub acquire_timeout_secs: Option<u64>,
    pub statement_timeout_ms: u32,
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            max_connections: 10,
            idle_timeout_secs: 30,
            acquire_timeout_secs: None,
            statement_timeout_ms: 60_000,
        }
    }
}

impl PoolSettings {
    /// Preset for a small auxiliary/side pool -- profile enrichment, marketplace
    /// reads, usage-grant writes, and similar low-traffic secondary connections:
    /// 5 connections, 60s idle timeout, 10s acquire timeout, 60s statement
    /// timeout. Consolidates the previously-duplicated `5 / 60 / Some(10)` inline
    /// literal so the numbers live in one place.
    pub fn side_pool() -> Self {
        Self {
            max_connections: 5,
            idle_timeout_secs: 60,
            acquire_timeout_secs: Some(10),
            ..Self::default()
        }
    }

    /// Preset for a service's primary request-serving pool: 20 connections, 60s
    /// idle timeout, 10s acquire timeout, 60s statement timeout. Consolidates the
    /// previously-duplicated `20 / 60 / Some(10)` inline literal.
    pub fn standard_service() -> Self {
        Self {
            max_connections: 20,
            idle_timeout_secs: 60,
            acquire_timeout_secs: Some(10),
            ..Self::default()
        }
    }
}

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("invalid postgres connection string: {0}")]
    InvalidUrl(sqlx::Error),

    #[error(transparent)]
    Connect(sqlx::Error),
}

/// URL-form pool constructor: parses a postgres connection URL, applies the
/// canonical `statement_timeout` / `idle_in_transaction_session_timeout`
/// connect options, and builds the pool with the given knobs.
pub async fn connect_pool(url: &str, settings: &PoolSettings) -> Result<PgPool, PoolError> {
    let statement_timeout = settings.statement_timeout_ms.to_string();
    let connect_opts: PgConnectOptions = url
        .parse::<PgConnectOptions>()
        .map_err(PoolError::InvalidUrl)?
        .options([
            ("statement_timeout", statement_timeout.as_str()),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);

    let mut pool_opts = PgPoolOptions::new()
        .max_connections(settings.max_connections)
        .idle_timeout(std::time::Duration::from_secs(settings.idle_timeout_secs));
    if let Some(secs) = settings.acquire_timeout_secs {
        pool_opts = pool_opts.acquire_timeout(std::time::Duration::from_secs(secs));
    }

    pool_opts
        .connect_with(connect_opts)
        .await
        .map_err(PoolError::Connect)
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

        let pool = connect_pool(
            &url,
            &PoolSettings {
                max_connections: cfg.max_connections,
                idle_timeout_secs: cfg.idle_timeout_secs,
                ..PoolSettings::default()
            },
        )
        .await
        .map_err(|e| match e {
            PoolError::InvalidUrl(e) | PoolError::Connect(e) => DatabaseError::ConnectionFailed(e),
        })?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
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

    #[test]
    fn side_pool_preset_values() {
        let s = PoolSettings::side_pool();
        assert_eq!(s.max_connections, 5);
        assert_eq!(s.idle_timeout_secs, 60);
        assert_eq!(s.acquire_timeout_secs, Some(10));
        // statement_timeout inherits the canonical 60s default.
        assert_eq!(s.statement_timeout_ms, 60_000);
    }

    #[test]
    fn standard_service_preset_values() {
        let s = PoolSettings::standard_service();
        assert_eq!(s.max_connections, 20);
        assert_eq!(s.idle_timeout_secs, 60);
        assert_eq!(s.acquire_timeout_secs, Some(10));
        assert_eq!(s.statement_timeout_ms, 60_000);
    }
}
