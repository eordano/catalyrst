use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Serialize, Serializer};
use sqlx::types::JsonValue;

use super::{
    ASSET_TYPE_COLLECTION_ITEM, ASSET_TYPE_ERC20, ASSET_TYPE_ERC721, ASSET_TYPE_USD_PEGGED_MANA,
};

fn ms<S: Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_i64(dt.timestamp_millis())
}

fn iso<S: Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true))
}

#[derive(Debug, Serialize)]
pub struct DbTradeListRow {
    pub id: String,
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(serialize_with = "iso")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "iso")]
    pub effective_since: DateTime<Utc>,
    #[serde(serialize_with = "iso")]
    pub expires_at: DateTime<Utc>,
    pub network: String,
    pub signature: String,
    pub signer: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub contract: String,
}

#[derive(Debug, Serialize)]
pub struct DbTrade {
    pub id: String,
    #[serde(rename = "chainId")]
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(rename = "createdAt", serialize_with = "ms")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "effectiveSince", serialize_with = "ms")]
    pub effective_since: DateTime<Utc>,
    #[serde(rename = "expiresAt", serialize_with = "ms")]
    pub expires_at: DateTime<Utc>,
    pub network: String,
    pub signature: String,
    pub signer: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub contract: String,
}

#[derive(Debug, Serialize)]
pub struct TradeAsset {
    #[serde(rename = "assetType")]
    pub asset_type: i32,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiary: Option<String>,
    #[serde(skip_serializing)]
    pub direction: String,
    pub extra: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TradeView {
    #[serde(flatten)]
    pub trade: DbTrade,
    pub sent: Vec<TradeAsset>,
    pub received: Vec<TradeAsset>,
}

#[derive(Debug, Serialize)]
pub struct PublicTradeAsset {
    #[serde(rename = "assetType")]
    pub asset_type: i32,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    pub extra: String,
    #[serde(rename = "amount", skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiary: Option<String>,
}

impl PublicTradeAsset {
    fn from_db(asset: &TradeAsset, with_beneficiary: bool) -> Self {
        let (amount, token_id, item_id) = match asset.asset_type {
            ASSET_TYPE_ERC20 | ASSET_TYPE_USD_PEGGED_MANA => (asset.amount.clone(), None, None),
            ASSET_TYPE_ERC721 => (None, asset.token_id.clone(), None),
            ASSET_TYPE_COLLECTION_ITEM => (None, None, asset.item_id.clone()),
            _ => (None, None, None),
        };
        PublicTradeAsset {
            asset_type: asset.asset_type,
            contract_address: asset.contract_address.clone(),
            extra: asset.extra.clone(),
            amount,
            token_id,
            item_id,
            beneficiary: if with_beneficiary {
                asset.beneficiary.clone()
            } else {
                None
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Trade {
    pub id: String,
    pub signer: String,
    pub signature: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub network: String,
    #[serde(rename = "chainId")]
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(rename = "createdAt", serialize_with = "ms")]
    pub created_at: DateTime<Utc>,
    pub sent: Vec<PublicTradeAsset>,
    pub received: Vec<PublicTradeAsset>,
    pub contract: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl Trade {
    pub(super) fn from_view(view: &TradeView) -> Self {
        Trade {
            id: view.trade.id.clone(),
            signer: view.trade.signer.clone(),
            signature: view.trade.signature.clone(),
            trade_type: view.trade.trade_type.clone(),
            network: view.trade.network.clone(),
            chain_id: view.trade.chain_id,
            checks: view.trade.checks.clone(),
            created_at: view.trade.created_at,
            sent: view
                .sent
                .iter()
                .map(|a| PublicTradeAsset::from_db(a, false))
                .collect(),
            received: view
                .received
                .iter()
                .map(|a| PublicTradeAsset::from_db(a, true))
                .collect(),
            contract: view.trade.contract.clone(),
            status: None,
        }
    }
}
