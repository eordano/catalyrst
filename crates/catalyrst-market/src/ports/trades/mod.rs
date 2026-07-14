mod component;
mod contracts;
mod create;
mod eip712;
mod events;
mod ownership;
mod types;

#[cfg(test)]
mod create_tests;
#[cfg(test)]
mod wire_tests;

pub use component::TradesComponent;
pub use contracts::{offchain_marketplace_v2, OffChainMarketplace};
pub use create::{
    create_trade, TradeChainAccess, TradeCreation, TradeCreationError,
    TRADE_TYPE_PUBLIC_ITEM_ORDER, TRADE_TYPE_PUBLIC_NFT_ORDER,
};
pub use eip712::signing_hash;
pub use ownership::RpcEndpoints;
pub use types::{DbTrade, DbTradeListRow, PublicTradeAsset, Trade, TradeAsset, TradeView};

#[cfg(test)]
use events::{bid_accepted_event, item_sold_event, AssetMeta};

const ASSET_TYPE_ERC20: i32 = 1;
const ASSET_TYPE_USD_PEGGED_MANA: i32 = 2;
const ASSET_TYPE_ERC721: i32 = 3;
const ASSET_TYPE_COLLECTION_ITEM: i32 = 4;
