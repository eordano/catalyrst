use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use sqlx::PgPool;

use super::contracts::{is_estate_chain, network_for_chain, offchain_marketplaces};
use super::eip712::{resolve_signature, SignatureError};
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
    #[serde(deserialize_with = "integral_u64")]
    pub uses: u64,
    #[serde(deserialize_with = "integral_i64")]
    pub expiration: i64,
    #[serde(deserialize_with = "integral_i64")]
    pub effective: i64,
    pub salt: String,
    #[serde(rename = "contractSignatureIndex", deserialize_with = "integral_u64")]
    pub contract_signature_index: u64,
    #[serde(rename = "signerSignatureIndex", deserialize_with = "integral_u64")]
    pub signer_signature_index: u64,
    #[serde(rename = "allowedRoot")]
    pub allowed_root: String,
    #[serde(rename = "allowedProof", default)]
    pub allowed_proof: Option<Vec<String>>,
    #[serde(rename = "externalChecks", default)]
    pub external_checks: Option<Vec<ExternalCheckInput>>,
}

/// Upstream validates these with ajv's `type: 'number'`, which does not care how a number was
/// spelled: `137` and `137.0` are one chain id, and a wallet that serialises through a float
/// must not be told its trade is malformed. A fractional value is still refused.
enum JsonNumber {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

impl<'de> Deserialize<'de> for JsonNumber {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let number = serde_json::Number::deserialize(deserializer)?;
        if let Some(value) = number.as_u64() {
            Ok(Self::Unsigned(value))
        } else if let Some(value) = number.as_i64() {
            Ok(Self::Signed(value))
        } else {
            number
                .as_f64()
                .map(Self::Float)
                .ok_or_else(|| serde::de::Error::custom("number is outside the supported range"))
        }
    }
}

impl JsonNumber {
    fn integral<E: serde::de::Error>(value: f64) -> Result<f64, E> {
        if value.is_finite() && value.fract() == 0.0 {
            Ok(value)
        } else {
            Err(E::custom(format!("{value} is not an integer")))
        }
    }

    fn into_i64<E: serde::de::Error>(self) -> Result<i64, E> {
        match self {
            JsonNumber::Unsigned(v) => {
                i64::try_from(v).map_err(|_| E::custom(format!("{v} does not fit in an i64")))
            }
            JsonNumber::Signed(v) => Ok(v),
            JsonNumber::Float(v) => {
                let v = Self::integral(v)?;
                if v >= -(2f64.powi(63)) && v < 2f64.powi(63) {
                    Ok(v as i64)
                } else {
                    Err(E::custom(format!("{v} does not fit in an i64")))
                }
            }
        }
    }

    fn into_u64<E: serde::de::Error>(self) -> Result<u64, E> {
        match self {
            JsonNumber::Unsigned(v) => Ok(v),
            JsonNumber::Signed(v) => {
                u64::try_from(v).map_err(|_| E::custom(format!("{v} is negative")))
            }
            JsonNumber::Float(v) => {
                let v = Self::integral(v)?;
                if (0.0..2f64.powi(64)).contains(&v) {
                    Ok(v as u64)
                } else {
                    Err(E::custom(format!("{v} does not fit in a u64")))
                }
            }
        }
    }
}

fn integral_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    JsonNumber::deserialize(d)?.into_i64()
}

fn integral_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    JsonNumber::deserialize(d)?.into_u64()
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
    #[serde(rename = "chainId", deserialize_with = "integral_i64")]
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
    let candidates = offchain_marketplaces(trade.chain_id);
    if candidates.is_empty() {
        return Err(TradeCreationError::UnsupportedChain(trade.chain_id));
    }
    let network = network_for_chain(trade.chain_id)
        .ok_or(TradeCreationError::UnsupportedChain(trade.chain_id))?;
    if !trade.network.eq_ignore_ascii_case(network) {
        return Err(TradeCreationError::NetworkMismatch {
            expected: network.to_string(),
            got: trade.network.clone(),
        });
    }
    validate_structure(trade)?;
    let matched = resolve_signature(trade, &candidates)
        .map_err(|e| TradeCreationError::InvalidSignature(e.to_string()))?;
    verify_sent_ownership(trade, chain).await?;
    insert_trade(
        pool,
        trade,
        network,
        matched.marketplace.address,
        matched.cancellation_digest.as_deref(),
    )
    .await
}

/// Trade head, assets and per-kind rows in one statement; a duplicate signature inserts nothing.
async fn insert_trade(
    pool: &PgPool,
    trade: &TradeCreation,
    network: &str,
    contract: &str,
    trade_digest: Option<&str>,
) -> Result<String, TradeCreationError> {
    let expires_at = ms_to_utc(trade.checks.expiration)?;
    let effective_since = ms_to_utc(trade.checks.effective)?;
    let checks = checks_json(&trade.checks).map_err(|e| {
        TradeCreationError::InvalidStructure(format!("checks are not serialisable: {e}"))
    })?;
    let mut rows = Vec::with_capacity(trade.sent.len() + trade.received.len());
    for (direction, assets) in [("sent", &trade.sent), ("received", &trade.received)] {
        for asset in assets {
            let (value, error) = match asset.asset_type {
                ASSET_TYPE_ERC721 => (&asset.token_id, "erc721 asset needs a tokenId"),
                ASSET_TYPE_ERC20 | ASSET_TYPE_USD_PEGGED_MANA => {
                    (&asset.amount, "fungible asset needs an amount")
                }
                ASSET_TYPE_COLLECTION_ITEM => (&asset.item_id, "collection item needs an itemId"),
                other => {
                    return Err(TradeCreationError::InvalidStructure(format!(
                        "unsupported asset type {other}"
                    )))
                }
            };
            let value = value
                .as_ref()
                .ok_or_else(|| TradeCreationError::InvalidStructure(error.to_owned()))?;
            rows.push(serde_json::json!({
                "direction": direction,
                "asset_type": asset.asset_type,
                "contract_address": asset.contract_address.to_lowercase(),
                "beneficiary": asset.beneficiary.as_ref().map(|b| b.to_lowercase()),
                "extra": asset.extra.as_deref().unwrap_or("0x"),
                "value": value,
            }));
        }
    }
    let inserted: Option<(String,)> = sqlx::query_as(CREATE_TRADE_SQL)
        .bind(network)
        .bind(trade.chain_id as i32)
        .bind(&trade.signature)
        .bind(hashed_signature(&trade.signature))
        .bind(&checks)
        .bind(trade.signer.to_lowercase())
        .bind(&trade.trade_type)
        .bind(expires_at)
        .bind(effective_since)
        .bind(contract)
        .bind(trade_digest)
        .bind(serde_json::Value::Array(rows))
        .fetch_optional(pool)
        .await?;
    let Some((trade_id,)) = inserted else {
        return Err(TradeCreationError::Duplicate);
    };
    Ok(trade_id)
}

const CREATE_TRADE_SQL: &str = "WITH t AS (
        INSERT INTO marketplace.trades
            (network, chain_id, signature, hashed_signature, checks, signer, type,
             expires_at, effective_since, contract, trade_digest)
        VALUES ($1, $2, $3, $4, $5, $6, $7::marketplace.trade_type, $8, $9, $10, $11)
        ON CONFLICT (hashed_signature) DO NOTHING
        RETURNING id
     ), input AS MATERIALIZED (
        SELECT gen_random_uuid() AS id, t.id AS trade_id, r.*
        FROM t, jsonb_to_recordset($12) AS r(direction text, asset_type smallint,
            contract_address text, beneficiary text, extra text, value text)
     ), inserted AS (
        INSERT INTO marketplace.trade_assets
            (id, trade_id, direction, asset_type, contract_address, beneficiary, extra)
        SELECT id, trade_id, direction::marketplace.asset_direction_type,
               asset_type, contract_address, beneficiary, extra FROM input
        RETURNING id
     ), erc721 AS (
        INSERT INTO marketplace.trade_assets_erc721 (asset_id, token_id)
        SELECT i.id, i.value FROM input i JOIN inserted USING (id) WHERE i.asset_type = 3
     ), erc20 AS (
        INSERT INTO marketplace.trade_assets_erc20 (asset_id, amount)
        SELECT i.id, i.value::numeric FROM input i JOIN inserted USING (id)
        WHERE i.asset_type IN (1, 2)
     ), item AS (
        INSERT INTO marketplace.trade_assets_item (asset_id, item_id)
        SELECT i.id, i.value FROM input i JOIN inserted USING (id) WHERE i.asset_type = 4
     )
     SELECT id::text FROM t";

#[cfg(test)]
#[path = "create_batch_tests.rs"]
mod batch_tests;

fn hashed_signature(signature: &str) -> String {
    use alloy_primitives::keccak256;
    format!("0x{:x}", keccak256(signature.as_bytes()))
}

/// The canonical camelCase JSON the `checks` column holds. Shared with the coupons port so a
/// stored trade's checks and a stored coupon's checks can never spell the same field two ways.
pub(crate) fn checks_json(
    checks: &TradeChecksInput,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(RawChecks::from(checks))
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
    #[serde(rename = "allowedProof", skip_serializing_if = "Option::is_none")]
    allowed_proof: Option<&'a [String]>,
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
            allowed_proof: checks.allowed_proof.as_deref(),
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
