//! `catalyrst-market` — entry point. Mirrors the wiring in
//! `marketplace-server/src/index.ts` + `service.ts`: builds component
//! instances, wires routes, then binds & serves.

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use catalyrst_market::config::Config;
use catalyrst_market::handlers;
use catalyrst_market::ports::{
    accounts::AccountsComponent,
    activity::ActivityComponent,
    analytics_day_data::AnalyticsDayDataComponent,
    bids::BidsComponent,
    catalog::CatalogComponent,
    collections::CollectionsComponent,
    contracts::ContractsComponent,
    items::ItemsComponent,
    nfts::NftsComponent,
    orders::OrdersComponent,
    owners::OwnersComponent,
    prices::PricesComponent,
    rankings::RankingsComponent,
    sales::SalesComponent,
    stats::StatsComponent,
    trades::TradesComponent,
    trendings::TrendingsComponent,
    user_assets::UserAssetsComponent,
    volume::VolumeComponent,
};
use catalyrst_market::AppStateInner;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_market=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;

    let dapps_read = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&cfg.dapps_read_database_url)
        .await?;

    sqlx::query(&format!("SET search_path TO {}, public", cfg.dapps_read_schema))
        .execute(&dapps_read)
        .await
        .ok();

    let pool = dapps_read.clone();

    // Activity composes Sales/Bids/Orders/Trades — its own instances; same pool.
    let activity_sales = Arc::new(SalesComponent::new(pool.clone()));
    let activity_bids = Arc::new(BidsComponent::new(pool.clone()));
    let activity_orders = Arc::new(OrdersComponent::new(pool.clone()));
    let activity_trades = Arc::new(TradesComponent::new(pool.clone()));
    let analytics_for_volume = AnalyticsDayDataComponent::new(pool.clone());

    let state = Arc::new(AppStateInner {
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
        trades: TradesComponent::new(pool.clone()),
        trendings: TrendingsComponent::new(pool.clone()),
        user_assets: UserAssetsComponent::new(pool.clone()),
        volume: VolumeComponent::new(analytics_for_volume),
    });

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        // contracts / collections / accounts / owners
        .route("/v1/contracts", get(handlers::contracts::get_contracts))
        .route("/v1/collections", get(handlers::collections::get_collections))
        .route("/v1/accounts", get(handlers::accounts::get_accounts))
        .route("/v1/owners", get(handlers::owners::get_owners))
        // catalog / nfts / items
        .route("/v1/catalog", get(handlers::catalog::get_catalog_v1))
        .route("/v2/catalog", get(handlers::catalog::get_catalog_v2))
        .route("/v1/nfts", get(handlers::nfts::get_nfts))
        .route("/v1/items", get(handlers::items::get_items))
        // user assets
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
        // trading reads
        .route("/v1/orders", get(handlers::orders::get_orders))
        .route("/v1/bids", get(handlers::bids::get_bids))
        .route("/v1/sales", get(handlers::sales::get_sales))
        .route("/v1/prices", get(handlers::prices::get_prices))
        .route("/v1/trendings", get(handlers::trendings::get_trendings))
        .route("/v1/rankings/{entity}/{timeframe}", get(handlers::rankings::get_rankings))
        .route("/v1/stats/{category}/{stat}", get(handlers::stats::get_stats))
        .route("/v1/volume/{timeframe}", get(handlers::volume::get_volume))
        .route("/v1/activity", get(handlers::activity::get_activity))
        .route("/v1/trades", get(handlers::trades::get_trades))
        .route("/v1/trades/{id}", get(handlers::trades::get_trade))
        .route(
            "/v1/trades/{hashed_signature}/accept",
            get(handlers::trades::get_trade_accepted_event),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-market listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
