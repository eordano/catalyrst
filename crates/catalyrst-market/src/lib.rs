pub mod auth_chain;
pub mod config;
pub mod dcl_schemas;
pub mod fed;
pub mod handlers;
pub mod http;
pub mod logic;
pub mod marketplace_contracts;
pub mod ports;
pub mod trades_sync;

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
use crate::ports::lists::ListsComponent;
use crate::ports::mana_rate::ManaUsdRateComponent;
use crate::ports::nfts::NftsComponent;
use crate::ports::orders::OrdersComponent;
use crate::ports::owners::OwnersComponent;
use crate::ports::prices::PricesComponent;
use crate::ports::rankings::RankingsComponent;
use crate::ports::sales::SalesComponent;
use crate::ports::shop_catalog::ShopCatalogComponent;
use crate::ports::stats::StatsComponent;
use crate::ports::trades::TradesComponent;
use crate::ports::trendings::TrendingsComponent;
use crate::ports::usage_grants::UsageGrantsComponent;
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
    pub lists: ListsComponent,
    pub mana_usd_rate: ManaUsdRateComponent,
    pub nfts: NftsComponent,
    pub orders: OrdersComponent,
    pub owners: OwnersComponent,
    pub prices: PricesComponent,
    pub rankings: RankingsComponent,
    pub sales: SalesComponent,
    pub shop_catalog: ShopCatalogComponent,
    pub stats: StatsComponent,
    pub trades: TradesComponent,
    pub trendings: TrendingsComponent,
    pub user_assets: UserAssetsComponent,

    pub usage_grants: UsageGrantsComponent,
    pub volume: VolumeComponent,
    pub pool: PgPool,
    pub replay: Arc<Replay>,
    pub limiter: Arc<RateLimiter>,
    pub domain: Eip712Domain,

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
        .route(
            "/v3/catalog/shop",
            get(handlers::shop_catalog::get_shop_catalog),
        )
        .route(
            "/v3/catalog/legacy",
            get(handlers::shop_catalog::get_legacy_catalog),
        )
        .route(
            "/v3/catalog/unified",
            get(handlers::shop_catalog::get_unified_catalog),
        )
        .route("/v3/catalog/items", get(handlers::items::get_catalog_items))
        .route(
            "/v3/catalog/importable",
            get(handlers::shop_catalog::get_importable_listings),
        )
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
        .route("/v1/lists", get(handlers::lists::get_lists))
        .route(
            "/v1/picks/{item_id}",
            post(handlers::picks::pick_unpick_in_bulk).delete(handlers::picks::unpick_everywhere),
        )
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

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SET search_path TO {}, public",
        cfg.dapps_read_schema
    )))
    .execute(&dapps_read)
    .await
    .ok();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SET search_path TO {}, public",
        cfg.dapps_schema
    )))
    .execute(&dapps_write)
    .await
    .ok();

    if let Err(e) = sqlx::migrate!("./migrations").run(&dapps_write).await {
        tracing::error!(error = %e, "federation migration failed");
        return Err(e.into());
    }

    spawn_mv_trades_refresh(dapps_write.clone());
    match &cfg.trades_sync_upstream_url {
        Some(url) => trades_sync::spawn_trades_upstream_sync(
            dapps_write.clone(),
            url.clone(),
            cfg.trades_sync_interval_secs,
        ),
        None => tracing::info!(
            "TRADES_SYNC_UPSTREAM_URL is empty: upstream trades freshness sync is off; \
             marketplace.trades stays at its last imported snapshot"
        ),
    }
    match &cfg.content_database_url {
        Some(url) => spawn_wearable_last_seen_refresh(dapps_write.clone(), url.clone()),
        None => tracing::info!(
            "CONTENT_PG_COMPONENT_PSQL_CONNECTION_STRING unset: wearable_last_seen refresher off; \
             sortBy=suggested ranks on whatever the table already holds"
        ),
    }

    let mana_usd_rate = ManaUsdRateComponent::new(
        cfg.price_base_url.clone(),
        cfg.mana_usd_fallback_rate,
        cfg.mana_oracle_max_staleness_secs,
    );
    mana_usd_rate
        .start(
            Duration::from_millis(cfg.mana_rate_startup_timeout_ms),
            Duration::from_millis(cfg.mana_rate_refresh_interval_ms.max(1)),
        )
        .await;

    let replay = Replay::new(dapps_write.clone())
        .await
        .context("failed to load federation replay state")?;
    let limiter = Arc::new(RateLimiter::new(120, Duration::from_secs(60)));

    let pool = dapps_read.clone();

    let grants_present = crate::ports::user_assets::usage_grants_present(&pool).await;

    let activity_sales = Arc::new(SalesComponent::new(pool.clone()));
    let activity_bids = Arc::new(BidsComponent::new(pool.clone()));
    let activity_orders = Arc::new(OrdersComponent::new(pool.clone()));

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
        lists: ListsComponent::new(pool.clone()).with_write(dapps_write.clone()),
        mana_usd_rate,
        nfts: NftsComponent::new(pool.clone()),
        orders: OrdersComponent::new(pool.clone()),
        owners: OwnersComponent::new(pool.clone()),
        prices: PricesComponent::new(pool.clone()),
        rankings: RankingsComponent::new(pool.clone()),
        sales: SalesComponent::new(pool.clone()),
        shop_catalog: ShopCatalogComponent::new(pool.clone()),
        stats: StatsComponent::new(pool.clone()),
        trades: TradesComponent::new(pool.clone(), cfg.trades_pagination),
        trendings: TrendingsComponent::new(pool.clone()),
        user_assets: UserAssetsComponent::new(pool.clone(), grants_present),
        usage_grants: UsageGrantsComponent::new(Some(pool.clone())),
        volume: VolumeComponent::new(analytics_for_volume),
        pool: dapps_write.clone(),
        replay,
        limiter,
        domain: market_domain(),
        admin_token: cfg.admin_token.clone(),
    }))
}

const MV_TRADES_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

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
            let mut refreshed = concurrent.is_ok();
            if let Err(e) = concurrent {
                tracing::debug!(error = %e, "concurrent mv_trades refresh failed; retrying plain");
                match sqlx::query("REFRESH MATERIALIZED VIEW marketplace.mv_trades")
                    .execute(&pool)
                    .await
                {
                    Ok(_) => refreshed = true,
                    Err(e) => tracing::warn!(error = %e, "mv_trades refresh failed"),
                }
            }
            if refreshed {
                if let Err(e) =
                    sqlx::query("SELECT pg_notify('catalyrst_market_dirty', 'mv_trades')")
                        .execute(&pool)
                        .await
                {
                    tracing::debug!(error = %e, "mv_trades dirty notify failed");
                }
            }
        }
    });
}

const WEARABLE_LAST_SEEN_REFRESH_INTERVAL: Duration = Duration::from_secs(600);

fn spawn_wearable_last_seen_refresh(dapps: PgPool, content_url: String) {
    tokio::spawn(async move {
        let content = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&content_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, "content DB unreachable: wearable_last_seen refresher off");
                return;
            }
        };
        let mut ticker = tokio::time::interval(WEARABLE_LAST_SEEN_REFRESH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match refresh_wearable_last_seen(&content, &dapps).await {
                Ok(0) => {
                    tracing::warn!("wearable_last_seen refresh: content DB returned no worn URNs")
                }
                Ok(n) => tracing::debug!(rows = n, "wearable_last_seen refreshed"),
                Err(e) => tracing::warn!(error = %e, "wearable_last_seen refresh failed"),
            }
        }
    });
}

async fn refresh_wearable_last_seen(content: &PgPool, dapps: &PgPool) -> Result<u64> {
    let rows: Vec<(String, chrono::NaiveDateTime)> = sqlx::query_as(
        "SELECT lower(array_to_string((string_to_array(w.urn, ':'))[1:6], ':')) AS urn,
                max(d.entity_timestamp) AS last_seen
         FROM deployments d,
              json_array_elements(d.entity_metadata->'v'->'avatars') a,
              json_array_elements_text(a->'avatar'->'wearables') w(urn)
         WHERE d.entity_type = 'profile' AND d.deleter_deployment IS NULL
           AND d.entity_timestamp > now() - interval '30 days'
         GROUP BY 1",
    )
    .fetch_all(content)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let (urns, seen): (Vec<String>, Vec<chrono::NaiveDateTime>) = rows.into_iter().unzip();
    sqlx::query(
        "INSERT INTO marketplace.wearable_last_seen (urn, last_seen, refreshed_at)
         SELECT u, ts, now() FROM unnest($1::text[], $2::timestamp[]) AS t(u, ts)
         ON CONFLICT (urn) DO UPDATE
           SET last_seen = EXCLUDED.last_seen, refreshed_at = now()",
    )
    .bind(&urns)
    .bind(&seen)
    .execute(dapps)
    .await?;
    sqlx::query("SELECT pg_notify('catalyrst_market_dirty', 'wearable_last_seen')")
        .execute(dapps)
        .await
        .ok();
    Ok(urns.len() as u64)
}
