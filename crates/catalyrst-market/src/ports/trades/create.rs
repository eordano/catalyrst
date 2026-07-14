use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};

use super::contracts::{is_estate_chain, network_for_chain, offchain_marketplace_v2};
use super::eip712::{verify_signature, SignatureError};
use super::ownership::{owner_of, OwnershipError, RpcEndpoints};
use super::{
    ASSET_TYPE_COLLECTION_ITEM, ASSET_TYPE_ERC20, ASSET_TYPE_ERC721, ASSET_TYPE_USD_PEGGED_MANA,
};

pub const TRADE_TYPE_BID: &str = "bid";
pub const TRADE_TYPE_PUBLIC_NFT_ORDER: &str = "public_nft_order";
pub const TRADE_TYPE_PUBLIC_ITEM_ORDER: &str = "public_item_order";

#[derive(Debug, Deserialize)]
pub struct ExternalCheckInput {
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    pub selector: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Deserialize)]
pub struct TradeChecksInput {
    pub uses: u64,
    pub expiration: i64,
    pub effective: i64,
    pub salt: String,
    #[serde(rename = "contractSignatureIndex")]
    pub contract_signature_index: u64,
    #[serde(rename = "signerSignatureIndex")]
    pub signer_signature_index: u64,
    #[serde(rename = "allowedRoot")]
    pub allowed_root: String,
    #[serde(rename = "externalChecks", default)]
    pub external_checks: Option<Vec<ExternalCheckInput>>,
}

#[derive(Debug, Deserialize)]
pub struct TradeAssetInput {
    #[serde(rename = "assetType")]
    pub asset_type: i32,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(default)]
    pub extra: Option<String>,
    #[serde(default)]
    pub beneficiary: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(rename = "tokenId", default)]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", default)]
    pub item_id: Option<String>,
}

impl TradeAssetInput {
    pub fn signed_value(&self) -> Result<String, SignatureError> {
        let value = match self.asset_type {
            ASSET_TYPE_ERC20 | ASSET_TYPE_USD_PEGGED_MANA => self.amount.as_ref(),
            ASSET_TYPE_ERC721 => self.token_id.as_ref(),
            ASSET_TYPE_COLLECTION_ITEM => self.item_id.as_ref(),
            other => {
                return Err(SignatureError::Malformed(format!(
                    "unsupported asset type {other}"
                )))
            }
        };
        value.cloned().ok_or_else(|| {
            SignatureError::Malformed(format!(
                "asset type {} is missing its value field",
                self.asset_type
            ))
        })
    }

    fn is_erc721(&self) -> bool {
        self.asset_type == ASSET_TYPE_ERC721
    }

    fn is_item(&self) -> bool {
        self.asset_type == ASSET_TYPE_COLLECTION_ITEM
    }

    fn is_fungible(&self) -> bool {
        matches!(
            self.asset_type,
            ASSET_TYPE_ERC20 | ASSET_TYPE_USD_PEGGED_MANA
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct TradeCreation {
    pub signer: String,
    pub signature: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub network: String,
    #[serde(rename = "chainId")]
    pub chain_id: i64,
    pub checks: TradeChecksInput,
    pub sent: Vec<TradeAssetInput>,
    pub received: Vec<TradeAssetInput>,
}

#[derive(Debug)]
pub enum TradeCreationError {
    Expired,
    EffectiveAfterExpiration,
    SignerMismatch,
    UnsupportedChain(i64),
    NetworkMismatch { expected: String, got: String },
    InvalidStructure(String),
    InvalidSignature(String),
    OwnershipUnverifiable(i64),
    OwnershipLookupFailed(String),
    NotTheOwner { owner: String },
    Duplicate,
    Db(sqlx::Error),
}

impl std::fmt::Display for TradeCreationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeCreationError::Expired => write!(f, "the trade has already expired"),
            TradeCreationError::EffectiveAfterExpiration => {
                write!(f, "the trade becomes effective after it expires")
            }
            TradeCreationError::SignerMismatch => {
                write!(f, "the authenticated address is not the trade signer")
            }
            TradeCreationError::UnsupportedChain(chain) => {
                write!(f, "no off-chain marketplace is deployed on chain {chain}")
            }
            TradeCreationError::NetworkMismatch { expected, got } => {
                write!(f, "chain belongs to network {expected}, not {got}")
            }
            TradeCreationError::InvalidStructure(why) => {
                write!(f, "invalid trade structure: {why}")
            }
            TradeCreationError::InvalidSignature(why) => write!(f, "{why}"),
            TradeCreationError::OwnershipUnverifiable(chain) => write!(
                f,
                "ownership cannot be verified: no rpc endpoint is configured for chain {chain}"
            ),
            TradeCreationError::OwnershipLookupFailed(why) => write!(f, "{why}"),
            TradeCreationError::NotTheOwner { owner } => write!(
                f,
                "the signer does not own this token; it belongs to {owner}"
            ),
            TradeCreationError::Duplicate => {
                write!(f, "a trade with this signature already exists")
            }
            TradeCreationError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl From<sqlx::Error> for TradeCreationError {
    fn from(e: sqlx::Error) -> Self {
        TradeCreationError::Db(e)
    }
}

fn validate_structure(trade: &TradeCreation) -> Result<(), TradeCreationError> {
    let invalid = |why: &str| Err(TradeCreationError::InvalidStructure(why.to_string()));
    if trade.sent.is_empty() || trade.received.is_empty() {
        return invalid("both sent and received must carry at least one asset");
    }
    match trade.trade_type.as_str() {
        TRADE_TYPE_BID => {
            if !trade.sent.iter().all(|a| a.is_fungible()) {
                return invalid("a bid must send only fungible assets");
            }
            if !trade.received.iter().all(|a| a.is_erc721() || a.is_item()) {
                return invalid("a bid must receive an nft or a collection item");
            }
        }
        TRADE_TYPE_PUBLIC_NFT_ORDER => {
            if trade.sent.len() != 1 || !trade.sent[0].is_erc721() {
                return invalid("a public nft order must send exactly one erc721");
            }
            if !trade.received.iter().all(|a| a.is_fungible()) {
                return invalid("a public nft order must receive only fungible assets");
            }
        }
        TRADE_TYPE_PUBLIC_ITEM_ORDER => {
            if trade.sent.len() != 1 || !trade.sent[0].is_item() {
                return invalid("a public item order must send exactly one collection item");
            }
            if !trade.received.iter().all(|a| a.is_fungible()) {
                return invalid("a public item order must receive only fungible assets");
            }
        }
        other => {
            return Err(TradeCreationError::InvalidStructure(format!(
                "unknown trade type {other}"
            )))
        }
    }
    if is_estate_chain(trade.chain_id)
        && trade
            .sent
            .iter()
            .chain(trade.received.iter())
            .any(|a| a.is_item())
    {
        return invalid("collection items are not tradeable on an estate chain");
    }
    Ok(())
}

// Only an ERC721 send is checked, matching marketplace-server: a bid sends
// fungibles the escrow settles, and an item order mints rather than transfers.
async fn verify_sent_ownership(
    trade: &TradeCreation,
    chain: Option<&TradeChainAccess<'_>>,
) -> Result<(), TradeCreationError> {
    let Some(asset) = trade.sent.first() else {
        return Ok(());
    };
    if !asset.is_erc721() {
        return Ok(());
    }
    let token_id = asset.token_id.as_ref().ok_or_else(|| {
        TradeCreationError::InvalidStructure("erc721 asset needs a tokenId".to_string())
    })?;
    let access = chain.ok_or(TradeCreationError::OwnershipUnverifiable(trade.chain_id))?;
    let owner = owner_of(
        access.http,
        access.endpoints,
        trade.chain_id,
        &asset.contract_address,
        token_id,
    )
    .await
    .map_err(|e| match e {
        OwnershipError::NotConfigured(chain) => TradeCreationError::OwnershipUnverifiable(chain),
        other => TradeCreationError::OwnershipLookupFailed(other.to_string()),
    })?;
    if !owner.to_string().eq_ignore_ascii_case(&trade.signer) {
        return Err(TradeCreationError::NotTheOwner {
            owner: owner.to_string(),
        });
    }
    Ok(())
}

fn ms_to_utc(ms: i64) -> Result<DateTime<Utc>, TradeCreationError> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .ok_or_else(|| TradeCreationError::InvalidStructure(format!("unrepresentable time {ms}")))
}

pub struct TradeChainAccess<'a> {
    pub http: &'a reqwest::Client,
    pub endpoints: &'a RpcEndpoints,
}

pub async fn create_trade(
    pool: &PgPool,
    trade: &TradeCreation,
    authenticated: &str,
    now_ms: i64,
    chain: Option<&TradeChainAccess<'_>>,
) -> Result<String, TradeCreationError> {
    if trade.checks.expiration < now_ms {
        return Err(TradeCreationError::Expired);
    }
    if trade.checks.expiration < trade.checks.effective {
        return Err(TradeCreationError::EffectiveAfterExpiration);
    }
    if !trade.signer.eq_ignore_ascii_case(authenticated) {
        return Err(TradeCreationError::SignerMismatch);
    }
    let marketplace = offchain_marketplace_v2(trade.chain_id)
        .ok_or(TradeCreationError::UnsupportedChain(trade.chain_id))?;
    let network = network_for_chain(trade.chain_id)
        .ok_or(TradeCreationError::UnsupportedChain(trade.chain_id))?;
    if !trade.network.eq_ignore_ascii_case(network) {
        return Err(TradeCreationError::NetworkMismatch {
            expected: network.to_string(),
            got: trade.network.clone(),
        });
    }
    validate_structure(trade)?;
    verify_signature(trade, &marketplace)
        .map_err(|e| TradeCreationError::InvalidSignature(e.to_string()))?;
    verify_sent_ownership(trade, chain).await?;

    let expires_at = ms_to_utc(trade.checks.expiration)?;
    let effective_since = ms_to_utc(trade.checks.effective)?;
    let checks = serde_json::to_value(RawChecks::from(&trade.checks)).map_err(|e| {
        TradeCreationError::InvalidStructure(format!("checks are not serialisable: {e}"))
    })?;

    let mut tx = pool.begin().await?;
    let inserted: Option<(String,)> = sqlx::query_as(
        "INSERT INTO marketplace.trades \
             (network, chain_id, signature, hashed_signature, checks, signer, type, \
              expires_at, effective_since, contract) \
         VALUES ($1, $2, $3, $4, $5, $6, $7::marketplace.trade_type, $8, $9, $10) \
         ON CONFLICT (hashed_signature) DO NOTHING \
         RETURNING id::text",
    )
    .bind(network)
    .bind(trade.chain_id as i32)
    .bind(&trade.signature)
    .bind(hashed_signature(&trade.signature))
    .bind(&checks)
    .bind(trade.signer.to_lowercase())
    .bind(&trade.trade_type)
    .bind(expires_at)
    .bind(effective_since)
    .bind(marketplace.address)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((trade_id,)) = inserted else {
        return Err(TradeCreationError::Duplicate);
    };

    for (direction, assets) in [("sent", &trade.sent), ("received", &trade.received)] {
        for asset in assets.iter() {
            insert_asset(&mut tx, &trade_id, direction, asset).await?;
        }
    }

    tx.commit().await?;
    Ok(trade_id)
}

async fn insert_asset(
    tx: &mut Transaction<'_, Postgres>,
    trade_id: &str,
    direction: &str,
    asset: &TradeAssetInput,
) -> Result<(), TradeCreationError> {
    let (asset_id,): (String,) = sqlx::query_as(
        "INSERT INTO marketplace.trade_assets \
             (trade_id, direction, asset_type, contract_address, beneficiary, extra) \
         VALUES ($1::uuid, $2::marketplace.asset_direction_type, $3, $4, $5, $6) \
         RETURNING id::text",
    )
    .bind(trade_id)
    .bind(direction)
    .bind(asset.asset_type as i16)
    .bind(asset.contract_address.to_lowercase())
    .bind(asset.beneficiary.as_ref().map(|b| b.to_lowercase()))
    .bind(asset.extra.clone().unwrap_or_else(|| "0x".to_string()))
    .fetch_one(&mut **tx)
    .await?;

    match asset.asset_type {
        ASSET_TYPE_ERC721 => {
            let token_id = asset.token_id.as_ref().ok_or_else(|| {
                TradeCreationError::InvalidStructure("erc721 asset needs a tokenId".to_string())
            })?;
            sqlx::query(
                "INSERT INTO marketplace.trade_assets_erc721 (asset_id, token_id) \
                 VALUES ($1::uuid, $2)",
            )
            .bind(&asset_id)
            .bind(token_id)
            .execute(&mut **tx)
            .await?;
        }
        ASSET_TYPE_ERC20 | ASSET_TYPE_USD_PEGGED_MANA => {
            let amount = asset.amount.as_ref().ok_or_else(|| {
                TradeCreationError::InvalidStructure("fungible asset needs an amount".to_string())
            })?;
            sqlx::query(
                "INSERT INTO marketplace.trade_assets_erc20 (asset_id, amount) \
                 VALUES ($1::uuid, $2::numeric)",
            )
            .bind(&asset_id)
            .bind(amount)
            .execute(&mut **tx)
            .await?;
        }
        ASSET_TYPE_COLLECTION_ITEM => {
            let item_id = asset.item_id.as_ref().ok_or_else(|| {
                TradeCreationError::InvalidStructure("collection item needs an itemId".to_string())
            })?;
            sqlx::query(
                "INSERT INTO marketplace.trade_assets_item (asset_id, item_id) \
                 VALUES ($1::uuid, $2)",
            )
            .bind(&asset_id)
            .bind(item_id)
            .execute(&mut **tx)
            .await?;
        }
        other => {
            return Err(TradeCreationError::InvalidStructure(format!(
                "unsupported asset type {other}"
            )))
        }
    }
    Ok(())
}

fn hashed_signature(signature: &str) -> String {
    use alloy_primitives::keccak256;
    format!("0x{:x}", keccak256(signature.as_bytes()))
}

#[derive(serde::Serialize)]
struct RawChecks<'a> {
    uses: u64,
    expiration: i64,
    effective: i64,
    salt: &'a str,
    #[serde(rename = "contractSignatureIndex")]
    contract_signature_index: u64,
    #[serde(rename = "signerSignatureIndex")]
    signer_signature_index: u64,
    #[serde(rename = "allowedRoot")]
    allowed_root: &'a str,
    #[serde(rename = "externalChecks")]
    external_checks: Vec<RawExternalCheck<'a>>,
}

#[derive(serde::Serialize)]
struct RawExternalCheck<'a> {
    #[serde(rename = "contractAddress")]
    contract_address: &'a str,
    selector: &'a str,
    value: &'a str,
    required: bool,
}

impl<'a> From<&'a TradeChecksInput> for RawChecks<'a> {
    fn from(checks: &'a TradeChecksInput) -> Self {
        RawChecks {
            uses: checks.uses,
            expiration: checks.expiration,
            effective: checks.effective,
            salt: &checks.salt,
            contract_signature_index: checks.contract_signature_index,
            signer_signature_index: checks.signer_signature_index,
            allowed_root: &checks.allowed_root,
            external_checks: checks
                .external_checks
                .iter()
                .flatten()
                .map(|c| RawExternalCheck {
                    contract_address: &c.contract_address,
                    selector: &c.selector,
                    value: c.value.as_deref().unwrap_or("0x"),
                    required: c.required,
                })
                .collect(),
        }
    }
}
