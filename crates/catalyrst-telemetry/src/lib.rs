pub mod handlers;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

pub struct IngestControl {
    pub enabled: AtomicBool,

    pub quotas: RwLock<HashMap<String, i64>>,

    pub counter_day: RwLock<String>,

    pub counters: RwLock<HashMap<String, i64>>,
}

impl IngestControl {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            quotas: RwLock::new(HashMap::new()),
            counter_day: RwLock::new(String::new()),
            counters: RwLock::new(HashMap::new()),
        }
    }

    pub fn admit(&self, project: &str) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        let limit = {
            let q = self.quotas.read().unwrap();
            match q.get(project) {
                Some(&l) => l,
                None => return true,
            }
        };
        let today = today_utc();
        {
            let day = self.counter_day.read().unwrap();
            if *day != today {
                drop(day);
                let mut day = self.counter_day.write().unwrap();
                if *day != today {
                    *day = today.clone();
                    self.counters.write().unwrap().clear();
                }
            }
        }
        let mut counters = self.counters.write().unwrap();
        let used = counters.entry(project.to_string()).or_insert(0);
        if *used >= limit {
            return false;
        }
        *used += 1;
        true
    }
}

fn today_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

pub struct AppStateInner {
    pub pool: PgPool,
    pub ingest: IngestControl,

    pub admin_token: Option<String>,
}

pub type AppState = Arc<AppStateInner>;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub admin_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: std::env::var("HTTP_SERVER_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: std::env::var("HTTP_SERVER_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5150),
            database_url: std::env::var("TELEMETRY_PG_CONNECTION_STRING")
                .context("missing TELEMETRY_PG_CONNECTION_STRING")?,
            admin_token: std::env::var("CATALYRST_TELEMETRY_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid TELEMETRY_PG_CONNECTION_STRING")?
        .options([("statement_timeout", "30000")]);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect telemetry pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run telemetry migrations")?;

    let ingest = IngestControl::new();

    if let Ok(Some((v,))) = sqlx::query_as::<_, (String,)>(
        "SELECT value FROM admin_settings WHERE key = 'ingest_enabled'",
    )
    .fetch_optional(&pool)
    .await
    {
        ingest.enabled.store(v != "false", Ordering::Relaxed);
    }
    if let Ok(rows) =
        sqlx::query_as::<_, (String, i64)>("SELECT project, daily_limit FROM project_quota")
            .fetch_all(&pool)
            .await
    {
        let mut q = ingest.quotas.write().unwrap();
        for (project, limit) in rows {
            q.insert(project, limit);
        }
    }

    Ok(Arc::new(AppStateInner {
        pool,
        ingest,
        admin_token: cfg.admin_token.clone(),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::ssr::page))
        .route("/events", get(handlers::ssr::page))
        .route("/issues/{fp}", get(handlers::ssr::page))
        .route("/metrics", get(handlers::ssr::page))
        .route("/metrics/stream", get(handlers::ssr::page))
        .route("/metrics/funnel", get(handlers::ssr::page))
        .route("/metrics/breakdown", get(handlers::ssr::page))
        .route("/health", get(handlers::ssr::page))
        .route("/flags", get(handlers::ssr::page))
        .route("/sql", get(handlers::ssr::page))
        .route("/session/{id}", get(handlers::ssr::page))
        .route("/dash/events", get(handlers::dashboard::events))
        .route("/dash/event/{id}", get(handlers::dashboard::event_detail))
        .route("/dash/stats", get(handlers::dashboard::stats))
        .route("/dash/metrics", get(handlers::dashboard::metrics))
        .route("/dash/health", get(handlers::dashboard::health))
        .route("/dash/funnel", get(handlers::dashboard::funnel))
        .route("/dash/breakdown", get(handlers::dashboard::breakdown))
        .route("/dash/flags", get(handlers::dashboard::flags))
        .route("/dash/sql", post(handlers::dashboard::sql_query))
        .route("/dash/story/{id}", get(handlers::dashboard::story))
        .route("/dash/session/{id}", get(handlers::dashboard::session))
        .route(
            "/dash/issue/state",
            post(handlers::dashboard::set_issue_state),
        )
        .route(
            "/dash/experiments",
            get(handlers::dashboard::experiments_get),
        )
        .route(
            "/dash/experiment",
            post(handlers::dashboard::experiment_set),
        )
        .route("/dash/admin/purge", post(handlers::admin::purge))
        .route("/dash/admin/ingest", post(handlers::admin::ingest_toggle))
        .route("/dash/admin/quota", post(handlers::admin::quota))
        .route(
            "/dash/admin/bulk-delete",
            post(handlers::admin::bulk_delete),
        )
        .route("/dash/admin/export", post(handlers::admin::export))
        .route("/dash/admin/audit", get(handlers::admin::audit_list))
        .route("/dash/admin/regroup", post(handlers::admin::regroup))
        .route("/dash/admin/release", post(handlers::admin::release))
        .route("/api/{project}/envelope/", post(handlers::sentry::envelope))
        .route("/api/{project}/envelope", post(handlers::sentry::envelope))
        .route("/api/{project}/store/", post(handlers::sentry::store))
        .route("/api/{project}/store", post(handlers::sentry::store))
        .route("/v1/batch", post(handlers::segment::batch))
        .route("/v1/import", post(handlers::segment::batch))
        .route("/v1/track", post(handlers::segment::single))
        .route("/v1/identify", post(handlers::segment::single))
        .route("/v1/page", post(handlers::segment::single))
        .route("/v1/screen", post(handlers::segment::single))
        .route("/v1/group", post(handlers::segment::single))
        .route("/v1/alias", post(handlers::segment::single))
}
