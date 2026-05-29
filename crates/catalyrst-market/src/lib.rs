pub mod auth_chain;
pub mod config;
pub mod dcl_schemas;
pub mod handlers;
pub mod http;
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
}

pub type AppState = Arc<AppStateInner>;
