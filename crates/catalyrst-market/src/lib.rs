//! catalyrst-market — REST API on top of the squid_marketplace schema.
//!
//! Mirrors `decentraland/marketplace-server` (Node.js) endpoint-by-endpoint
//! and response-shape-by-response-shape so existing clients are byte-identical.
//! Reads only — write endpoints (trades, favorites, transak, wert) are out of
//! scope until the federation ADR is written.

pub mod config;
pub mod dcl_schemas;
pub mod http;
pub mod handlers;
pub mod logic;
pub mod marketplace_contracts;
pub mod ports;

use std::sync::Arc;

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

/// Schema name that the squid indexer writes into. Mirrors
/// `marketplace-server/src/constants.ts:MARKETPLACE_SQUID_SCHEMA`.
pub const MARKETPLACE_SQUID_SCHEMA: &str = "squid_marketplace";

/// Builder-server materialized-view schema. Mirrors
/// `marketplace-server/src/constants.ts:BUILDER_SERVER_TABLE_SCHEMA`.
pub const BUILDER_SERVER_TABLE_SCHEMA: &str = "marketplace";

/// Shared component container — mirrors `types.ts:AppComponents`.
/// Every new port adds a field here; the binary in main.rs builds the
/// instances and registers routes against them.
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
}

pub type AppState = Arc<AppStateInner>;
