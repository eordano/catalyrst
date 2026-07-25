mod component;
mod events;
mod types;

#[cfg(test)]
mod wire_tests;

pub use component::TradesComponent;
pub use types::{DbTrade, DbTradeListRow, PublicTradeAsset, Trade, TradeAsset, TradeView};

#[cfg(test)]
use events::{bid_accepted_event, item_sold_event, AssetMeta};

const ASSET_TYPE_ERC20: i32 = 1;
const ASSET_TYPE_USD_PEGGED_MANA: i32 = 2;
const ASSET_TYPE_ERC721: i32 = 3;
const ASSET_TYPE_COLLECTION_ITEM: i32 = 4;
