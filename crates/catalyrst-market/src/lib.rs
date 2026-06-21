pub mod auth_chain;
pub mod config;
pub mod dcl_schemas;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod logic;
pub mod marketplace_contracts;
pub mod ports;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use axum::routing::{get, post};
use axum::Router;
use catalyrst_db::database::{Database, DatabaseConfig};
use catalyrst_fed::sig::Eip712Domain;
use catalyrst_fed::RateLimiter;
use sqlx::PgPool;

use crate::config::Config;
use crate::fed::market_domain;
use crate::fed::replay::Replay;
use crate::ports::accounts::AccountsComponent;
use crate::ports::activity::ActivityComponent;
use crate::ports::analytics_day_data::AnalyticsDayDataComponent;
use crate::ports::bids::BidsComponent;
use crate::ports::catalog::CatalogComponent;
use crate::ports::collections::CollectionsComponent;
use crate::ports::contracts::ContractsComponent;
use crate::ports::items::ItemsComponent;
use crate::ports::nfts::NftsComponent;
use crate::ports::orders::OrdersComponent;
use crate::ports::owners::OwnersComponent;
use crate::ports::prices::PricesComponent;
use crate::ports::rankings::RankingsComponent;
use crate::ports::sales::SalesComponent;
use crate::ports::stats::StatsComponent;
use crate::ports::trades::TradesComponent;
use crate::ports::trendings::TrendingsComponent;
use crate::ports::user_assets::UserAssetsComponent;
use crate::ports::volume::VolumeComponent;

pub const MARKETPLACE_SQUID_SCHEMA: &str = "squid_marketplace";

pub const BUILDER_SERVER_TABLE_SCHEMA: &str = "marketplace";

pub struct AppStateInner {
    pub accounts: AccountsComponent,
    pub activity: ActivityComponent,
    pub analytics_day_data: AnalyticsDayDataComponent,
    pub bids: BidsComponent,
    pub catalog: CatalogComponent,
    pub collections: CollectionsComponent,
    pub contracts: ContractsComponent,
    pub items: ItemsComponent,
    pub nfts: NftsComponent,
    pub orders: OrdersComponent,
    pub owners: OwnersComponent,
    pub prices: PricesComponent,
    pub rankings: RankingsComponent,
    pub sales: SalesComponent,
    pub stats: StatsComponent,
    pub trades: TradesComponent,
    pub trendings: TrendingsComponent,
    pub user_assets: UserAssetsComponent,
    pub volume: VolumeComponent,
    pub pool: PgPool,
    pub replay: Arc<Replay>,
    pub limiter: Arc<RateLimiter>,
    pub domain: Eip712Domain,
    /// Bearer token gating the §4 admin moderation routes; `None` ⇒ fail closed.
    pub admin_token: Option<String>,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/v1/contracts", get(handlers::contracts::get_contracts))
        .route(
            "/v1/collections",
            get(handlers::collections::get_collections),
        )
        .route("/v1/accounts", get(handlers::accounts::get_accounts))
        .route("/v1/owners", get(handlers::owners::get_owners))
        .route("/v1/catalog", get(handlers::catalog::get_catalog_v1))
        .route("/v2/catalog", get(handlers::catalog::get_catalog_v2))
        .route("/v1/nfts", get(handlers::nfts::get_nfts))
        .route("/v1/items", get(handlers::items::get_items))
        .route(
            "/v1/users/{address}/wearables",
            get(handlers::user_assets::wearables::get_user_wearables),
        )
        .route(
            "/v1/users/{address}/wearables/grouped",
            get(handlers::user_assets::wearables::get_user_grouped_wearables),
        )
        .route(
            "/v1/users/{address}/wearables/urn-token",
            get(handlers::user_assets::wearables::get_user_wearables_urn_token),
        )
        .route(
            "/v1/users/{address}/emotes",
            get(handlers::user_assets::emotes::get_user_emotes),
        )
        .route(
            "/v1/users/{address}/emotes/grouped",
            get(handlers::user_assets::emotes::get_user_grouped_emotes),
        )
        .route(
            "/v1/users/{address}/emotes/urn-token",
            get(handlers::user_assets::emotes::get_user_emotes_urn_token),
        )
        .route(
            "/v1/users/{address}/names",
            get(handlers::user_assets::names::get_user_names),
        )
        .route(
            "/v1/users/{address}/names/names-only",
            get(handlers::user_assets::names::get_user_names_only),
        )
        .route("/v1/orders", get(handlers::orders::get_orders))
        .route("/v1/bids", get(handlers::bids::get_bids))
        .route("/v1/sales", get(handlers::sales::get_sales))
        .route("/v1/prices", get(handlers::prices::get_prices))
        .route("/v1/trendings", get(handlers::trendings::get_trendings))
        .route(
            "/v1/rankings/{entity}/{timeframe}",
            get(handlers::rankings::get_rankings),
        )
        .route(
            "/v1/stats/{category}/{stat}",
            get(handlers::stats::get_stats),
        )
        .route("/v1/volume/{timeframe}", get(handlers::volume::get_volume))
        .route("/v1/activity", get(handlers::activity::get_activity))
        .route("/v1/trades", get(handlers::trades::get_trades))
        .route("/v1/trades/{id}", get(handlers::trades::get_trade))
        .route(
            "/v1/trades/{hashed_signature}/accept",
            get(handlers::trades::get_trade_accepted_event),
        )
        .route("/v1/federation/bid", post(handlers::federation::place_bid))
        .route(
            "/v1/federation/bid/cancel",
            post(handlers::federation::cancel_bid),
        )
        .route(
            "/v1/federation/bid/accept",
            post(handlers::federation::accept_bid),
        )
        .route(
            "/v1/federation/order",
            post(handlers::federation::create_order),
        )
        .route(
            "/v1/federation/order/cancel",
            post(handlers::federation::cancel_order),
        )
        .route(
            "/v1/federation/trade",
            post(handlers::federation::record_trade),
        )
        .route("/v1/federation/bids", get(handlers::federation::list_bids))
        .route(
            "/v1/federation/orders",
            get(handlers::federation::list_orders),
        )
        .route(
            "/v1/federation/trades",
            get(handlers::federation::list_trades),
        )
        .route(
            "/federation/market/snapshot",
            get(handlers::federation::snapshot),
        )
        .route(
            "/federation/market/changes",
            get(handlers::federation::changes),
        )
        // ---- §4 admin moderation (bearer-gated; CATALYRST_MARKET_ADMIN_TOKEN) ----
        .route(
            "/v1/admin/moderation/flags",
            get(handlers::admin::list_flags),
        )
        .route(
            "/v1/admin/moderation/{kind}/{hash}/flag",
            post(handlers::admin::set_flag).delete(handlers::admin::clear_flag),
        )
        .route("/v1/admin/disputes", get(handlers::admin::list_disputes))
        .route(
            "/v1/admin/disputes/{trade_hash}/open",
            post(handlers::admin::open_dispute),
        )
        .route(
            "/v1/admin/disputes/{trade_hash}/resolve",
            post(handlers::admin::resolve_dispute),
        )
        .route(
            "/v1/admin/listings/{kind}/{hash}/force-cancel",
            post(handlers::admin::force_cancel),
        )
        .route("/v1/admin/audit", get(handlers::admin::list_audit))
        // DoS guard (parity-preserving): bound per-request time and total
        // in-flight requests so a single caller can't loop an expensive endpoint
        // — notably the unpaginated /v1/trades 43MB full-table dump — to exhaust
        // process memory / the DB pool. Limits are generous; normal traffic is
        // unaffected (the load test peaked well under these).
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .layer(tower::limit::GlobalConcurrencyLimitLayer::new(256))
}

fn db_config_from_url(url_str: &str, max_connections: u32) -> Result<DatabaseConfig> {
    let url =
        url::Url::parse(url_str).with_context(|| format!("invalid postgres URL: {url_str}"))?;
    if url.scheme() != "postgres" && url.scheme() != "postgresql" {
        return Err(anyhow!(
            "unexpected URL scheme {:?}, want postgres://",
            url.scheme()
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("postgres URL missing host: {url_str}"))?
        .to_string();
    let port = url.port().unwrap_or(5432);
    let user = if url.username().is_empty() {
        "postgres".to_string()
    } else {
        url::form_urlencoded::parse(url.username().as_bytes())
            .map(|(k, _)| k.into_owned())
            .next()
            .unwrap_or_else(|| url.username().to_string())
    };
    let password = url
        .password()
        .map(|p| {
            url::form_urlencoded::parse(p.as_bytes())
                .map(|(k, _)| k.into_owned())
                .next()
                .unwrap_or_else(|| p.to_string())
        })
        .unwrap_or_default();
    let database = url.path().trim_start_matches('/').to_string();
    if database.is_empty() {
        return Err(anyhow!("postgres URL missing database name: {url_str}"));
    }
    Ok(DatabaseConfig {
        host,
        port,
        database,
        user,
        password,
        max_connections,
        idle_timeout_secs: 30,
        query_timeout_secs: 60,
    })
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let dapps_db = Database::connect(&db_config_from_url(&cfg.dapps_database_url, 10)?)
        .await
        .context("failed to connect dapps pool")?;
    let dapps_read_db = Database::connect(&db_config_from_url(&cfg.dapps_read_database_url, 20)?)
        .await
        .context("failed to connect dapps_read pool")?;
    let favorites_db = Database::connect(&db_config_from_url(&cfg.favorites_database_url, 10)?)
        .await
        .context("failed to connect favorites pool")?;

    let _ = (&dapps_db, &favorites_db);

    let dapps_read = dapps_read_db.pool().clone();
    let dapps_write = dapps_db.pool().clone();

    sqlx::query(&format!(
        "SET search_path TO {}, public",
        cfg.dapps_read_schema
    ))
    .execute(&dapps_read)
    .await
    .ok();
    sqlx::query(&format!("SET search_path TO {}, public", cfg.dapps_schema))
        .execute(&dapps_write)
        .await
        .ok();

    if let Err(e) = sqlx::migrate!("./migrations").run(&dapps_write).await {
        tracing::error!(error = %e, "federation migration failed");
        return Err(e.into());
    }

    spawn_mv_trades_refresh(dapps_write.clone());

    let replay = Replay::new(dapps_write.clone())
        .await
        .context("failed to load federation replay state")?;
    let limiter = Arc::new(RateLimiter::new(120, Duration::from_secs(60)));

    let pool = dapps_read.clone();

    let activity_sales = Arc::new(SalesComponent::new(pool.clone()));
    let activity_bids = Arc::new(BidsComponent::new(pool.clone()));
    let activity_orders = Arc::new(OrdersComponent::new(pool.clone()));
    // Activity reader path never serves /v1/trades, so pagination is irrelevant here.
    let activity_trades = Arc::new(TradesComponent::new(pool.clone(), false));
    let analytics_for_volume = AnalyticsDayDataComponent::new(pool.clone());

    Ok(Arc::new(AppStateInner {
        accounts: AccountsComponent::new(pool.clone()),
        activity: ActivityComponent::new(
            activity_sales.clone(),
            activity_bids.clone(),
            activity_orders.clone(),
            activity_trades.clone(),
        ),
        analytics_day_data: AnalyticsDayDataComponent::new(pool.clone()),
        bids: BidsComponent::new(pool.clone()),
        catalog: CatalogComponent::new(pool.clone()),
        collections: CollectionsComponent::new(pool.clone()),
        contracts: ContractsComponent::new(pool.clone()),
        items: ItemsComponent::new(pool.clone()),
        nfts: NftsComponent::new(pool.clone()),
        orders: OrdersComponent::new(pool.clone()),
        owners: OwnersComponent::new(pool.clone()),
        prices: PricesComponent::new(pool.clone()),
        rankings: RankingsComponent::new(pool.clone()),
        sales: SalesComponent::new(pool.clone()),
        stats: StatsComponent::new(pool.clone()),
        trades: TradesComponent::new(pool.clone(), cfg.trades_pagination),
        trendings: TrendingsComponent::new(pool.clone()),
        user_assets: UserAssetsComponent::new(pool.clone()),
        volume: VolumeComponent::new(analytics_for_volume),
        pool: dapps_write.clone(),
        replay,
        limiter,
        domain: market_domain(),
        admin_token: cfg.admin_token.clone(),
    }))
}

/// How often the off-chain trades materialized view is refreshed. Upstream
/// marketplace-server refreshes `mv_trades` from DB triggers on every write to
/// the trade / nft / item / squid_trades tables; catalyrst's squid mirror has
/// no such trigger surface, so we refresh on a fixed interval instead. 30s
/// keeps price / isOnSale / trade* in step with the continuously-syncing squid
/// mirror and flips expired listings to `cancelled` promptly.
const MV_TRADES_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Keep `marketplace.mv_trades` current. The view is created (and populated)
/// by migration 0004; this task re-derives it so newly-indexed trades, supply
/// changes, recorded cancellations/executions, and expiries are reflected.
///
/// `REFRESH ... CONCURRENTLY` requires the unique index on `id` and lets reads
/// continue against the prior snapshot during the refresh. If a CONCURRENT
/// refresh is rejected (e.g. the view has never been populated), it falls back
/// to a plain refresh. Any error is logged and retried on the next tick rather
/// than crashing the service — a stale view degrades gracefully.
fn spawn_mv_trades_refresh(pool: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(MV_TRADES_REFRESH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let concurrent =
                sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY marketplace.mv_trades")
                    .execute(&pool)
                    .await;
            if let Err(e) = concurrent {
                tracing::debug!(error = %e, "concurrent mv_trades refresh failed; retrying plain");
                if let Err(e) = sqlx::query("REFRESH MATERIALIZED VIEW marketplace.mv_trades")
                    .execute(&pool)
                    .await
                {
                    tracing::warn!(error = %e, "mv_trades refresh failed");
                }
            }
        }
    });
}
