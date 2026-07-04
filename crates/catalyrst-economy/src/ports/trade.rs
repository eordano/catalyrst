//! Off-chain marketplace (Marketplace v3) signed-trade acceptance.
//!
//! Derivation provenance — everything in this module is derived from the
//! mirrored Solidity at
//! `decentraland/offchain-marketplace-contract` (github):
//!   * struct layout:            src/common/CommonTypes.sol (ExternalCheck,
//!     Checks) + src/marketplace/MarketplaceTypes.sol (Asset, Trade);
//!   * accept()/uses semantics:  src/marketplace/Marketplace.sol
//!     (`accept(Trade[])`, `_verifyTrade`, `getTradeId`; a signature carries
//!     `checks.uses` total uses across ALL callers, and each accept consumes
//!     exactly one use);
//!   * checks verification:      src/common/Verifications.sol (expiration /
//!     effective / indexes / allowedRoot Merkle gate / externalChecks);
//!   * EIP-712 hashing:          src/common/CommonTypesHashing.sol +
//!     src/marketplace/MarketplaceTypesHashing.sol — NON-standard: `sent`
//!     assets are hashed as `AssetWithoutBeneficiary` (the caller picks the
//!     sent-asset beneficiary at accept time, outside the signature) and
//!     `Checks` is hashed WITHOUT `allowedProof`;
//!   * domain:                   src/common/EIP712.sol — a modified OZ EIP712
//!     whose domain is `EIP712Domain(string name,string version,address
//!     verifyingContract,bytes32 salt)` with **salt = bytes32(chainId)** (no
//!     chainId field), constructed as
//!     EIP712("DecentralandMarketplacePolygon", "1.0.0") for the Polygon
//!     deployment 0x540fb08eDb56AaE562864B390542C97F562825BA
//!     (decentraland-transactions src/contracts/offChainMarketplace.ts);
//!   * beneficiary defaulting:   Marketplace.sol `_transferAssets` — a zero
//!     beneficiary on a sent asset resolves to the caller; we set it
//!     explicitly to the relayer (same move the upstream dapp makes in
//!     marketplace/webapp/src/utils/trades.ts `getOnChainTrade`);
//!   * DB-JSON -> struct value conversion (ms->s timestamps, salt/allowedRoot
//!     left-padding to bytes32, ''/'0x' empty bytes):
//!     marketplace/webapp/src/utils/trades.ts `generateTradeValues`.
//!
//! The golden test vector at the bottom is a REAL mainnet-signed open trade
//! (marketplace.trades id 1bbe7d78-dd71-4cbe-9085-70d679d3ad11): the digest
//! computed here recovers its actual signer, proving domain + struct hashing
//! + value conversion end-to-end against production data.

use alloy::primitives::{address, keccak256, Address, Bytes, B256, U256};
use alloy::sol;
use alloy::sol_types::SolCall;
use serde::Deserialize;

use crate::http::errors::ApiError;
use crate::ports::abi::ERC721_TRANSFER_TOPIC0;
use crate::ports::broker::BrokerCall;
use crate::ports::signer::ReceiptLog;

/// DecentralandMarketplacePolygon 1.0.0 (Polygon mainnet, chain 137).
pub const OFFCHAIN_MARKETPLACE_POLYGON: Address =
    address!("0x540fb08eDb56AaE562864B390542C97F562825BA");

pub const OFFCHAIN_MARKETPLACE_EIP712_NAME: &str = "DecentralandMarketplacePolygon";
pub const OFFCHAIN_MARKETPLACE_EIP712_VERSION: &str = "1.0.0";

/// MANA on Polygon (DecentralandMarketplacePolygon `manaAddress`).
pub const MANA_POLYGON: Address = address!("0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4");

// Asset types — DecentralandMarketplace{Ethereum,Polygon}AssetTypes.sol.
pub const ASSET_TYPE_ERC20: i64 = 1;
pub const ASSET_TYPE_USD_PEGGED_MANA: i64 = 2;
pub const ASSET_TYPE_ERC721: i64 = 3;
pub const ASSET_TYPE_COLLECTION_ITEM: i64 = 4;

sol! {
    /// Mirrors src/common/CommonTypes.sol `ExternalCheck`.
    #[derive(Debug)]
    struct SolExternalCheck {
        address contractAddress;
        bytes4 selector;
        bytes value;
        bool required;
    }

    /// Mirrors src/common/CommonTypes.sol `Checks`.
    #[derive(Debug)]
    struct SolChecks {
        uint256 uses;
        uint256 expiration;
        uint256 effective;
        bytes32 salt;
        uint256 contractSignatureIndex;
        uint256 signerSignatureIndex;
        bytes32 allowedRoot;
        bytes32[] allowedProof;
        SolExternalCheck[] externalChecks;
    }

    /// Mirrors src/marketplace/MarketplaceTypes.sol `Asset`.
    #[derive(Debug)]
    struct SolAsset {
        uint256 assetType;
        address contractAddress;
        uint256 value;
        address beneficiary;
        bytes extra;
    }

    /// Mirrors src/marketplace/MarketplaceTypes.sol `Trade`.
    #[derive(Debug)]
    struct SolTrade {
        address signer;
        bytes signature;
        SolChecks checks;
        SolAsset[] sent;
        SolAsset[] received;
    }

    /// Mirrors src/marketplace/Marketplace.sol `accept(Trade[] _trades)`.
    function accept(SolTrade[] _trades) external;
}

// ---------------------------------------------------------------------------
// EIP-712 hashing (contract-faithful, NON-standard: see module docs)
// ---------------------------------------------------------------------------

// keccak256("ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)")
// — src/common/CommonTypesHashing.sol
const EXTERNAL_CHECK_TYPE_HASH: [u8; 32] =
    hex_literal("8d4afe924d276922e1a624d4cc4d5b316cb369a5d290db2fae6417ec282d01f8");

// keccak256("Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)")
// — src/common/CommonTypesHashing.sol (note: NO allowedProof)
const CHECKS_TYPE_HASH: [u8; 32] =
    hex_literal("cae85973b802c2104c84d94b18a0a8a13a0576322547fe2fab563e83849ce641");

// keccak256("AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)")
// — src/marketplace/MarketplaceTypesHashing.sol
const ASSET_WO_BENEFICIARY_TYPE_HASH: [u8; 32] =
    hex_literal("7be57332caf51c5f0f0fa0e7c362534d22d81c0bee1ffac9b573acd336e032bd");

// keccak256("Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)")
const ASSET_TYPE_HASH: [u8; 32] =
    hex_literal("e5f9e1ebc316d1bde562c77f47da7dc2cccb903eb04f9b82e29212b96f9e57e1");

// keccak256("Trade(Checks checks,AssetWithoutBeneficiary[] sent,Asset[] received)Asset(...)AssetWithoutBeneficiary(...)Checks(...)ExternalCheck(...)")
// — src/marketplace/MarketplaceTypesHashing.sol
const TRADE_TYPE_HASH: [u8; 32] =
    hex_literal("1bb41340c6ec0467bb14b59212e1189437e71660f2ef919bda2be2f2065dfe6c");

// keccak256("EIP712Domain(string name,string version,address verifyingContract,bytes32 salt)")
// — src/common/EIP712.sol (modified OZ: chainId lives in `salt`)
const DOMAIN_TYPE_HASH: [u8; 32] =
    hex_literal("36c25de3e541d5d970f66e4210d728721220fff5c077cc6cd008b3a0c62adab7");

const fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("bad hex in const"),
    }
}

const fn hex_literal(s: &str) -> [u8; 32] {
    let bytes = s.as_bytes();
    assert!(bytes.len() == 64, "const hex literal must be 32 bytes");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (hex_val(bytes[2 * i]) << 4) | hex_val(bytes[2 * i + 1]);
        i += 1;
    }
    out
}

fn word_address(a: Address) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a.as_slice());
    w
}

fn word_u256(v: U256) -> [u8; 32] {
    v.to_be_bytes::<32>()
}

fn hash_external_checks(checks: &[SolExternalCheck]) -> B256 {
    let mut cat = Vec::with_capacity(checks.len() * 32);
    for c in checks {
        let mut enc = Vec::with_capacity(5 * 32);
        enc.extend_from_slice(&EXTERNAL_CHECK_TYPE_HASH);
        enc.extend_from_slice(&word_address(c.contractAddress));
        // bytes4 is abi.encode'd right-padded in its 32-byte word
        let mut sel = [0u8; 32];
        sel[..4].copy_from_slice(c.selector.as_slice());
        enc.extend_from_slice(&sel);
        enc.extend_from_slice(keccak256(&c.value).as_slice());
        enc.extend_from_slice(&word_u256(U256::from(c.required as u8)));
        cat.extend_from_slice(keccak256(&enc).as_slice());
    }
    keccak256(&cat)
}

fn hash_checks(c: &SolChecks) -> B256 {
    let mut enc = Vec::with_capacity(9 * 32);
    enc.extend_from_slice(&CHECKS_TYPE_HASH);
    enc.extend_from_slice(&word_u256(c.uses));
    enc.extend_from_slice(&word_u256(c.expiration));
    enc.extend_from_slice(&word_u256(c.effective));
    enc.extend_from_slice(c.salt.as_slice());
    enc.extend_from_slice(&word_u256(c.contractSignatureIndex));
    enc.extend_from_slice(&word_u256(c.signerSignatureIndex));
    enc.extend_from_slice(c.allowedRoot.as_slice());
    // allowedProof is intentionally NOT hashed (caller-supplied data)
    enc.extend_from_slice(hash_external_checks(&c.externalChecks).as_slice());
    keccak256(&enc)
}

fn hash_assets(assets: &[SolAsset], with_beneficiary: bool) -> B256 {
    let mut cat = Vec::with_capacity(assets.len() * 32);
    for a in assets {
        let mut enc = Vec::with_capacity(6 * 32);
        enc.extend_from_slice(if with_beneficiary {
            &ASSET_TYPE_HASH
        } else {
            &ASSET_WO_BENEFICIARY_TYPE_HASH
        });
        enc.extend_from_slice(&word_u256(a.assetType));
        enc.extend_from_slice(&word_address(a.contractAddress));
        enc.extend_from_slice(&word_u256(a.value));
        enc.extend_from_slice(keccak256(&a.extra).as_slice());
        if with_beneficiary {
            enc.extend_from_slice(&word_address(a.beneficiary));
        }
        cat.extend_from_slice(keccak256(&enc).as_slice());
    }
    keccak256(&cat)
}

pub fn hash_trade(t: &SolTrade) -> B256 {
    let mut enc = Vec::with_capacity(4 * 32);
    enc.extend_from_slice(&TRADE_TYPE_HASH);
    enc.extend_from_slice(hash_checks(&t.checks).as_slice());
    enc.extend_from_slice(hash_assets(&t.sent, false).as_slice());
    enc.extend_from_slice(hash_assets(&t.received, true).as_slice());
    keccak256(&enc)
}

fn domain_separator(chain_id: u64, verifying_contract: Address) -> B256 {
    let mut enc = Vec::with_capacity(5 * 32);
    enc.extend_from_slice(&DOMAIN_TYPE_HASH);
    enc.extend_from_slice(keccak256(OFFCHAIN_MARKETPLACE_EIP712_NAME.as_bytes()).as_slice());
    enc.extend_from_slice(keccak256(OFFCHAIN_MARKETPLACE_EIP712_VERSION.as_bytes()).as_slice());
    enc.extend_from_slice(&word_address(verifying_contract));
    // salt = bytes32(chainId): the modified-OZ domain's replay protection
    enc.extend_from_slice(&word_u256(U256::from(chain_id)));
    keccak256(&enc)
}

pub fn trade_digest(t: &SolTrade, chain_id: u64, verifying_contract: Address) -> B256 {
    let mut enc = Vec::with_capacity(2 + 64);
    enc.extend_from_slice(&[0x19, 0x01]);
    enc.extend_from_slice(domain_separator(chain_id, verifying_contract).as_slice());
    enc.extend_from_slice(hash_trade(t).as_slice());
    keccak256(&enc)
}

// ---------------------------------------------------------------------------
// Wire payload (what the credits worker forwards from catalyrst-market
// GET /v1/trades/{id}) and its validation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeChecksIn {
    pub uses: u64,
    /// milliseconds since epoch (DB convention); converted to seconds on-chain
    pub expiration: i64,
    /// milliseconds since epoch
    pub effective: i64,
    pub salt: String,
    pub contract_signature_index: u64,
    pub signer_signature_index: u64,
    #[serde(default)]
    pub allowed_root: Option<String>,
    #[serde(default)]
    pub allowed_proof: Option<Vec<String>>,
    #[serde(default)]
    pub external_checks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeAssetIn {
    pub asset_type: i64,
    pub contract_address: String,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub token_id: Option<String>,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub extra: Option<String>,
    #[serde(default)]
    pub beneficiary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeIn {
    pub id: String,
    pub signer: String,
    pub signature: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub chain_id: Option<u64>,
    pub checks: TradeChecksIn,
    pub sent: Vec<TradeAssetIn>,
    pub received: Vec<TradeAssetIn>,
    #[serde(default)]
    pub contract: Option<String>,
    /// catalyrst-market mv status; informational here (the credits worker
    /// pre-flights it; the contract is the authority at broadcast time).
    #[serde(default)]
    pub status: Option<String>,
}

/// What kind of asset the accepted trade delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDelivery {
    /// public_nft_order: an existing ERC721 moves signer -> caller.
    Nft { token_id: U256 },
    /// public_item_order: the collection mints a new token to the caller.
    Item { item_id: U256 },
}

#[derive(Debug, Clone)]
pub struct ValidatedTrade {
    pub signer: Address,
    pub hashed_signature: B256,
    pub collection: Address,
    pub price_wei: U256,
    pub delivery: TradeDelivery,
    /// Contract-ready struct. `sent[0].beneficiary` is Address::ZERO here and
    /// is set to the caller (relayer) by `build_trade_accept` — that field is
    /// NOT covered by the signature (AssetWithoutBeneficiary hashing).
    pub onchain: SolTrade,
}

pub struct TradeExpectations {
    pub collection: Address,
    pub price_wei: U256,
    pub chain_id: u64,
    pub relayer: Address,
    pub now_ms: i64,
}

fn err(msg: impl Into<String>) -> ApiError {
    ApiError::InvalidTransaction(msg.into())
}

fn parse_addr(label: &str, raw: &str) -> Result<Address, ApiError> {
    raw.trim()
        .parse::<Address>()
        .map_err(|e| err(format!("trade {label}: bad address {raw:?}: {e}")))
}

fn parse_hex_bytes(label: &str, raw: Option<&str>) -> Result<Vec<u8>, ApiError> {
    match raw.map(str::trim) {
        None | Some("") | Some("0x") => Ok(Vec::new()),
        Some(s) => {
            let h = s.strip_prefix("0x").unwrap_or(s);
            if h.len() % 2 != 0 {
                return Err(err(format!("trade {label}: odd-length hex {s:?}")));
            }
            alloy::hex::decode(h).map_err(|e| err(format!("trade {label}: bad hex {s:?}: {e}")))
        }
    }
}

/// ethers `hexZeroPad(value, 32)` semantics: LEFT-pad short hex to 32 bytes
/// (the dapp signs salts like "0x988c1638e5" this way).
fn parse_bytes32_padded(label: &str, raw: Option<&str>) -> Result<B256, ApiError> {
    let bytes = parse_hex_bytes(label, raw)?;
    if bytes.len() > 32 {
        return Err(err(format!("trade {label}: longer than 32 bytes")));
    }
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(B256::from(out))
}

fn parse_u256_dec(label: &str, raw: &str) -> Result<U256, ApiError> {
    U256::from_str_radix(raw.trim(), 10)
        .map_err(|e| err(format!("trade {label}: not a decimal integer {raw:?}: {e}")))
}

const MS_PER_SEC: i64 = 1000;

/// Refuse trades that expire within this window: the tx would land too close
/// to the on-chain `Expired` check to be worth relayer gas.
const EXPIRY_SAFETY_SECS: i64 = 30;

/// Validate a trade payload against the contract's accept() semantics and the
/// checkout's expectations. Fail-closed: anything this relayer cannot prove
/// it satisfies (allowlists, external checks, exotic asset sets, USD-pegged
/// pricing) is refused.
pub fn validate_trade(
    body: &TradeIn,
    expect: &TradeExpectations,
) -> Result<ValidatedTrade, ApiError> {
    // --- venue / chain ---
    if let Some(chain) = body.chain_id {
        if chain != expect.chain_id {
            return Err(err(format!(
                "trade is for chain {chain}, this broker buys on chain {}",
                expect.chain_id
            )));
        }
    }
    match body.contract.as_deref() {
        None => {}
        Some(c) => {
            if parse_addr("contract", c)? != OFFCHAIN_MARKETPLACE_POLYGON {
                return Err(err(format!(
                    "trade is bound to venue {c}, only DecentralandMarketplacePolygon \
                     {OFFCHAIN_MARKETPLACE_POLYGON:#x} is supported (the EIP-712 domain \
                     differs per venue)"
                )));
            }
        }
    }

    // --- trade type ---
    let is_item_order = match body.trade_type.as_str() {
        "public_nft_order" => false,
        "public_item_order" => true,
        other => {
            return Err(err(format!(
                "trade type {other:?} is not purchasable (public_nft_order/public_item_order only)"
            )))
        }
    };

    // --- checks ---
    let c = &body.checks;
    if c.uses == 0 {
        return Err(err("trade checks.uses is 0: the signature is unusable"));
    }
    if !is_item_order && c.uses != 1 {
        return Err(err(format!(
            "public_nft_order with uses={} — a signed NFT order sells one token; refusing \
             multi-use NFT orders",
            c.uses
        )));
    }
    let now_s = expect.now_ms / MS_PER_SEC;
    let expiration_s = c.expiration / MS_PER_SEC;
    let effective_s = c.effective / MS_PER_SEC;
    if expiration_s <= now_s + EXPIRY_SAFETY_SECS {
        return Err(err("trade is expired (or expires too soon to accept safely)"));
    }
    if effective_s > now_s {
        return Err(err("trade is not effective yet"));
    }
    let allowed_root = parse_bytes32_padded("checks.allowedRoot", c.allowed_root.as_deref())?;
    if allowed_root != B256::ZERO {
        return Err(err(
            "trade requires a Merkle allowlist proof (checks.allowedRoot set) that this \
             relayer cannot satisfy; refusing",
        ));
    }
    if c.allowed_proof.as_ref().is_some_and(|p| !p.is_empty()) {
        return Err(err("trade carries an allowedProof but no allowlist support here"));
    }
    if !c.external_checks.is_empty() {
        return Err(err(
            "trade requires external checks (caller-scoped balanceOf/ownerOf/custom calls) \
             that cannot be guaranteed for the relayer; refusing",
        ));
    }
    let salt = parse_bytes32_padded("checks.salt", Some(c.salt.as_str()))?;

    // --- received: exactly one ERC20 MANA payment at the expected price ---
    if body.received.len() != 1 {
        return Err(err(format!(
            "trade wants {} received assets; only a single MANA payment is supported",
            body.received.len()
        )));
    }
    let recv = &body.received[0];
    if recv.asset_type == ASSET_TYPE_USD_PEGGED_MANA {
        return Err(err(
            "trade is priced in USD-pegged MANA (assetType 2): the final MANA amount is \
             oracle-dependent at execution time, so the charge basis cannot be pinned; refusing",
        ));
    }
    if recv.asset_type != ASSET_TYPE_ERC20 {
        return Err(err(format!(
            "trade received asset has assetType {}, expected 1 (ERC20)",
            recv.asset_type
        )));
    }
    let recv_contract = parse_addr("received.contractAddress", &recv.contract_address)?;
    if recv_contract != MANA_POLYGON {
        return Err(err(format!(
            "trade wants payment in token {recv_contract:#x}, only MANA \
             {MANA_POLYGON:#x} is supported"
        )));
    }
    let amount = recv
        .amount
        .as_deref()
        .ok_or_else(|| err("trade received MANA asset is missing `amount`"))?;
    let price = parse_u256_dec("received.amount", amount)?;
    if price.is_zero() {
        return Err(err("trade price is 0 MANA; refusing a free 'sale'"));
    }
    if price != expect.price_wei {
        return Err(err(format!(
            "trade price {price} wei does not match the pinned charge basis {} wei",
            expect.price_wei
        )));
    }
    let recv_beneficiary = match recv.beneficiary.as_deref().map(str::trim) {
        None | Some("") => Address::ZERO,
        Some(b) => parse_addr("received.beneficiary", b)?,
    };
    let recv_extra = parse_hex_bytes("received.extra", recv.extra.as_deref())?;

    // --- sent: exactly one ERC721 (nft order) / collection item (item order) ---
    if body.sent.len() != 1 {
        return Err(err(format!(
            "trade sends {} assets; only single-asset trades are supported",
            body.sent.len()
        )));
    }
    let sent = &body.sent[0];
    let sent_contract = parse_addr("sent.contractAddress", &sent.contract_address)?;
    if sent_contract != expect.collection {
        return Err(err(format!(
            "trade sells {sent_contract:#x} but this purchase is for collection {:#x}",
            expect.collection
        )));
    }
    let (delivery, sent_value) = if is_item_order {
        if sent.asset_type != ASSET_TYPE_COLLECTION_ITEM {
            return Err(err(format!(
                "public_item_order sent asset has assetType {}, expected 4 (COLLECTION_ITEM)",
                sent.asset_type
            )));
        }
        let item_id = sent
            .item_id
            .as_deref()
            .ok_or_else(|| err("trade item asset is missing `itemId`"))?;
        let v = parse_u256_dec("sent.itemId", item_id)?;
        (TradeDelivery::Item { item_id: v }, v)
    } else {
        if sent.asset_type != ASSET_TYPE_ERC721 {
            return Err(err(format!(
                "public_nft_order sent asset has assetType {}, expected 3 (ERC721)",
                sent.asset_type
            )));
        }
        let token_id = sent
            .token_id
            .as_deref()
            .ok_or_else(|| err("trade NFT asset is missing `tokenId`"))?;
        let v = parse_u256_dec("sent.tokenId", token_id)?;
        (TradeDelivery::Nft { token_id: v }, v)
    };
    let sent_extra = parse_hex_bytes("sent.extra", sent.extra.as_deref())?;
    // The sent-asset beneficiary is NOT part of the signature (hashed as
    // AssetWithoutBeneficiary); the caller sets it at accept time. Any value
    // in the stored payload is ignored — build_trade_accept pins it to the
    // relayer so custody matches the other broker modes (relayer -> escrow).

    // --- signer + signature ---
    let signer = parse_addr("signer", &body.signer)?;
    let sig_bytes = parse_hex_bytes("signature", Some(body.signature.as_str()))?;
    if sig_bytes.len() != 65 {
        return Err(err(format!(
            "trade signature is {} bytes; only 65-byte EOA signatures are supported \
             (ERC-1271 contract signers are refused fail-closed)",
            sig_bytes.len()
        )));
    }

    let onchain = SolTrade {
        signer,
        signature: Bytes::from(sig_bytes.clone()),
        checks: SolChecks {
            uses: U256::from(c.uses),
            expiration: U256::from(expiration_s.max(0) as u64),
            effective: U256::from(effective_s.max(0) as u64),
            salt,
            contractSignatureIndex: U256::from(c.contract_signature_index),
            signerSignatureIndex: U256::from(c.signer_signature_index),
            allowedRoot: allowed_root,
            allowedProof: vec![],
            externalChecks: vec![],
        },
        sent: vec![SolAsset {
            assetType: U256::from(sent.asset_type as u64),
            contractAddress: sent_contract,
            value: sent_value,
            beneficiary: Address::ZERO,
            extra: Bytes::from(sent_extra),
        }],
        received: vec![SolAsset {
            assetType: U256::from(recv.asset_type as u64),
            contractAddress: recv_contract,
            value: price,
            beneficiary: recv_beneficiary,
            extra: Bytes::from(recv_extra),
        }],
    };

    // Recover the EIP-712 signer locally BEFORE any broadcast: if this does
    // not match, the on-chain accept() would revert with InvalidSignature —
    // and it also proves our encoding reproduces exactly what was signed.
    let digest = trade_digest(&onchain, expect.chain_id, OFFCHAIN_MARKETPLACE_POLYGON);
    let sig = alloy::primitives::Signature::from_raw(&sig_bytes)
        .map_err(|e| err(format!("trade signature is malformed: {e}")))?;
    let recovered = sig
        .recover_address_from_prehash(&digest)
        .map_err(|e| err(format!("trade signature recovery failed: {e}")))?;
    if recovered != signer {
        return Err(err(format!(
            "trade signature recovers to {recovered:#x}, not the declared signer \
             {signer:#x}; refusing (payload does not match what was signed)"
        )));
    }

    Ok(ValidatedTrade {
        signer,
        hashed_signature: keccak256(&sig_bytes),
        collection: sent_contract,
        price_wei: price,
        delivery,
        onchain,
    })
}

/// Encode `accept([trade])` with the sent-asset beneficiary pinned to the
/// caller (the relayer), so the NFT lands on the relayer and the existing
/// escrow-forward leg takes custody exactly like the other broker modes.
pub fn build_trade_accept(validated: &ValidatedTrade, caller: Address) -> BrokerCall {
    let mut trade = validated.onchain.clone();
    for asset in &mut trade.sent {
        asset.beneficiary = caller;
    }
    BrokerCall {
        to: OFFCHAIN_MARKETPLACE_POLYGON,
        data: Bytes::from(acceptCall { _trades: vec![trade] }.abi_encode()),
    }
}

/// Extract the tokenId of the ERC721 `Transfer(from -> to)` emitted by the
/// accepted trade (from = the trade signer for nft orders; from = 0x0 for
/// item orders, which mint).
pub fn transferred_token_id_from_logs(
    logs: &[ReceiptLog],
    collection: Address,
    from: Address,
    to: Address,
) -> Option<U256> {
    let topic0 = B256::from(ERC721_TRANSFER_TOPIC0);
    let from_topic = B256::from_slice(&[&[0u8; 12][..], from.as_slice()].concat());
    let to_topic = B256::from_slice(&[&[0u8; 12][..], to.as_slice()].concat());
    for log in logs {
        if log.address == collection
            && log.topics.len() == 4
            && log.topics[0] == topic0
            && log.topics[1] == from_topic
            && log.topics[2] == to_topic
        {
            return Some(U256::from_be_bytes(log.topics[3].0));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolCall;

    /// REAL open trade from marketplace.trades
    /// (id 1bbe7d78-dd71-4cbe-9085-70d679d3ad11): sells token 283 of
    /// 0xe9e8... for 1 MANA, signed on Polygon mainnet by 0x02d0bb59...
    fn fixture_json() -> serde_json::Value {
        serde_json::json!({
            "id": "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
            "signer": "0x02d0bb59a5f04a12d883751dc1605e15b4959b7e",
            "signature": "0x2860a680deb41ba57ee26d6972c21d49d6cca25c74613ca04b9ed15d48a154f205fd3554d71836277e9d3f0143a62afad6f5c1636a7cd3d8f691dc4b0d8ccd011b",
            "type": "public_nft_order",
            "network": "MATIC",
            "chainId": 137,
            "checks": {
                "salt": "0x199a4082c5",
                "uses": 1,
                "effective": 1733927535914i64,
                "expiration": 1798783200000i64,
                "allowedRoot": "0x",
                "externalChecks": [],
                "signerSignatureIndex": 0,
                "contractSignatureIndex": 0
            },
            "createdAt": 1733927542000i64,
            "sent": [
                {
                    "assetType": 3,
                    "contractAddress": "0xe9e86941b23fbe9d8f4dd0c5b7e5f89722936878",
                    "extra": "",
                    "tokenId": "283"
                }
            ],
            "received": [
                {
                    "assetType": 1,
                    "contractAddress": "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
                    "extra": "",
                    "amount": "1000000000000000000",
                    "beneficiary": "0x02d0bb59a5f04a12d883751dc1605e15b4959b7e"
                }
            ],
            "contract": "0x540fb08eDb56AaE562864B390542C97F562825BA",
            "status": "open"
        })
    }

    fn fixture() -> TradeIn {
        serde_json::from_value(fixture_json()).expect("fixture parses")
    }

    fn expectations() -> TradeExpectations {
        TradeExpectations {
            collection: address!("0xe9e86941b23fbe9d8f4dd0c5b7e5f89722936878"),
            price_wei: U256::from(1_000_000_000_000_000_000u64),
            chain_id: 137,
            relayer: address!("0x1111111111111111111111111111111111111111"),
            now_ms: 1_751_700_000_000, // 2025-07-05-ish, inside the window
        }
    }

    #[test]
    fn type_hash_constants_match_their_type_strings() {
        assert_eq!(
            EXTERNAL_CHECK_TYPE_HASH,
            keccak256("ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)").0
        );
        assert_eq!(
            CHECKS_TYPE_HASH,
            keccak256("Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)").0
        );
        assert_eq!(
            ASSET_WO_BENEFICIARY_TYPE_HASH,
            keccak256("AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)").0
        );
        assert_eq!(
            ASSET_TYPE_HASH,
            keccak256("Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)").0
        );
        assert_eq!(
            TRADE_TYPE_HASH,
            keccak256("Trade(Checks checks,AssetWithoutBeneficiary[] sent,Asset[] received)Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)").0
        );
        assert_eq!(
            DOMAIN_TYPE_HASH,
            keccak256("EIP712Domain(string name,string version,address verifyingContract,bytes32 salt)").0
        );
    }

    #[test]
    fn golden_digest_recovers_the_real_mainnet_signer() {
        let v = validate_trade(&fixture(), &expectations()).expect("real trade validates");
        // Digest independently cross-checked with python eth_account
        // encode_typed_data over the same domain/types/message.
        let digest = trade_digest(&v.onchain, 137, OFFCHAIN_MARKETPLACE_POLYGON);
        assert_eq!(
            alloy::hex::encode(digest),
            "d4d6a86e2a1f0ab327b88353ef9cbd59ddde578a73fdcc176d8b07564c6f7718"
        );
        assert_eq!(
            v.signer,
            address!("0x02d0bb59a5f04a12d883751dc1605e15b4959b7e")
        );
        assert_eq!(
            alloy::hex::encode(v.hashed_signature),
            // keccak256(signature) == marketplace.trades.hashed_signature
            "7a97bfb784c3559a5a7d251ea3889a124d7a91803c717d6674dfc2a3c95f91a8"
        );
        assert_eq!(v.delivery, TradeDelivery::Nft { token_id: U256::from(283u64) });
        assert_eq!(v.price_wei, U256::from(1_000_000_000_000_000_000u64));
    }

    #[test]
    fn accept_calldata_matches_the_hand_computed_abi_bytes() {
        // Expected bytes computed independently with python eth_abi:
        // selector = keccak("accept((address,bytes,(uint256,uint256,uint256,
        // bytes32,uint256,uint256,bytes32,bytes32[],(address,bytes4,bytes,
        // bool)[]),(uint256,address,uint256,address,bytes)[],(uint256,address,
        // uint256,address,bytes)[])[])")[..4] = 961a547e
        let v = validate_trade(&fixture(), &expectations()).unwrap();
        let call = build_trade_accept(&v, expectations().relayer);
        assert_eq!(call.to, OFFCHAIN_MARKETPLACE_POLYGON);
        let expected = concat!(
            "961a547e",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "00000000000000000000000002d0bb59a5f04a12d883751dc1605e15b4959b7e",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "0000000000000000000000000000000000000000000000000000000000000120",
            "0000000000000000000000000000000000000000000000000000000000000280",
            "0000000000000000000000000000000000000000000000000000000000000380",
            "0000000000000000000000000000000000000000000000000000000000000041",
            "2860a680deb41ba57ee26d6972c21d49d6cca25c74613ca04b9ed15d48a154f2",
            "05fd3554d71836277e9d3f0143a62afad6f5c1636a7cd3d8f691dc4b0d8ccd01",
            "1b00000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "000000000000000000000000000000000000000000000000000000006b3740e0",
            "000000000000000000000000000000000000000000000000000000006759a26f",
            "000000000000000000000000000000000000000000000000000000199a4082c5",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000120",
            "0000000000000000000000000000000000000000000000000000000000000140",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000003",
            "000000000000000000000000e9e86941b23fbe9d8f4dd0c5b7e5f89722936878",
            "000000000000000000000000000000000000000000000000000000000000011b",
            "0000000000000000000000001111111111111111111111111111111111111111",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "000000000000000000000000a1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
            "0000000000000000000000000000000000000000000000000de0b6b3a7640000",
            "00000000000000000000000002d0bb59a5f04a12d883751dc1605e15b4959b7e",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert_eq!(alloy::hex::encode(&call.data), expected);

        // and the alloy sol! types (generated from the mirrored solidity)
        // round-trip the same bytes
        let decoded = acceptCall::abi_decode(&call.data).unwrap();
        assert_eq!(decoded._trades.len(), 1);
        let t = &decoded._trades[0];
        assert_eq!(t.signer, v.signer);
        assert_eq!(t.checks.uses, U256::from(1u64));
        assert_eq!(t.checks.expiration, U256::from(1_798_783_200u64));
        assert_eq!(t.checks.effective, U256::from(1_733_927_535u64));
        assert_eq!(t.sent[0].value, U256::from(283u64));
        assert_eq!(t.sent[0].beneficiary, expectations().relayer);
        assert_eq!(t.received[0].value, U256::from(1_000_000_000_000_000_000u64));
    }

    fn with(f: impl FnOnce(&mut serde_json::Value)) -> TradeIn {
        let mut v = fixture_json();
        f(&mut v);
        serde_json::from_value(v).expect("mutated fixture parses")
    }

    #[test]
    fn wrong_expected_price_is_refused() {
        let mut e = expectations();
        e.price_wei = U256::from(2u64);
        assert!(validate_trade(&fixture(), &e).is_err());
    }

    #[test]
    fn bids_are_refused() {
        let t = with(|v| v["type"] = "bid".into());
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn wrong_venue_and_wrong_chain_are_refused() {
        // OffChainMarketplaceV2 (0xa40b...) has a different EIP-712 domain
        let t = with(|v| v["contract"] = "0xa40b1d129b8906888720686f3a01921ddf37716f".into());
        assert!(validate_trade(&t, &expectations()).is_err());
        let t = with(|v| v["chainId"] = 1.into());
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn expired_and_not_yet_effective_are_refused() {
        let mut e = expectations();
        e.now_ms = 1_798_783_200_000; // exactly at expiration
        assert!(validate_trade(&fixture(), &e).is_err());
        let mut e = expectations();
        e.now_ms = 1_733_927_000_000; // before effective
        assert!(validate_trade(&fixture(), &e).is_err());
    }

    #[test]
    fn allowlists_and_external_checks_are_refused_fail_closed() {
        let t = with(|v| {
            v["checks"]["allowedRoot"] =
                "0x00000000000000000000000000000000000000000000000000000000000000ff".into()
        });
        assert!(validate_trade(&t, &expectations()).is_err());
        let t = with(|v| {
            v["checks"]["externalChecks"] = serde_json::json!([{
                "contractAddress": "0x2a187453064356c898cae034eaed119e1663acb8",
                "selector": "0x70a08231",
                "value": "0x",
                "required": true
            }])
        });
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn usd_pegged_mana_and_non_mana_payment_are_refused() {
        let t = with(|v| v["received"][0]["assetType"] = 2.into());
        assert!(validate_trade(&t, &expectations()).is_err());
        let t = with(|v| {
            v["received"][0]["contractAddress"] =
                "0x0000000000000000000000000000000000000001".into()
        });
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn multi_asset_trades_are_refused() {
        let t = with(|v| {
            let extra = v["sent"][0].clone();
            v["sent"].as_array_mut().unwrap().push(extra);
        });
        assert!(validate_trade(&t, &expectations()).is_err());
        let t = with(|v| {
            let extra = v["received"][0].clone();
            v["received"].as_array_mut().unwrap().push(extra);
        });
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn multi_use_nft_orders_and_zero_uses_are_refused() {
        let t = with(|v| v["checks"]["uses"] = 2.into());
        assert!(validate_trade(&t, &expectations()).is_err());
        let t = with(|v| v["checks"]["uses"] = 0.into());
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn wrong_collection_is_refused() {
        let mut e = expectations();
        e.collection = address!("0x0000000000000000000000000000000000000009");
        assert!(validate_trade(&fixture(), &e).is_err());
    }

    #[test]
    fn tampered_payload_fails_signature_recovery() {
        // same signature, different token: recovery must not yield the signer
        let t = with(|v| v["sent"][0]["tokenId"] = "284".into());
        let err = validate_trade(&t, &expectations()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("recovers to"), "got: {msg}");
        // and a declared signer that did not sign is refused the same way
        let t = with(|v| v["signer"] = "0x1111111111111111111111111111111111111111".into());
        assert!(validate_trade(&t, &expectations()).is_err());
    }

    #[test]
    fn item_order_maps_to_collection_item_delivery() {
        // structural check only (no real signed item-order fixture): validate
        // fails at signature recovery, not before, when the shape is right
        let t = with(|v| {
            v["type"] = "public_item_order".into();
            v["sent"][0] = serde_json::json!({
                "assetType": 4,
                "contractAddress": "0xe9e86941b23fbe9d8f4dd0c5b7e5f89722936878",
                "extra": "",
                "itemId": "7"
            });
        });
        let err = validate_trade(&t, &expectations()).unwrap_err();
        assert!(
            format!("{err}").contains("recovers to"),
            "shape must validate up to the signature: {err}"
        );
    }

    #[test]
    fn erc1271_style_signatures_are_refused() {
        let t = with(|v| v["signature"] = "0x1234".into());
        let err = validate_trade(&t, &expectations()).unwrap_err();
        assert!(format!("{err}").contains("65-byte"));
    }

    #[test]
    fn transfer_extraction_requires_the_exact_from_to_pair() {
        let collection = address!("0xe9e86941b23fbe9d8f4dd0c5b7e5f89722936878");
        let seller = address!("0x02d0bb59a5f04a12d883751dc1605e15b4959b7e");
        let relayer = address!("0x1111111111111111111111111111111111111111");
        let other = address!("0x2222222222222222222222222222222222222222");
        let topic0 = B256::from(ERC721_TRANSFER_TOPIC0);
        let pad = |a: Address| B256::from_slice(&[&[0u8; 12][..], a.as_slice()].concat());
        let token = B256::from_slice(&U256::from(283u64).to_be_bytes::<32>());
        let log = |from: Address, to: Address| ReceiptLog {
            address: collection,
            topics: vec![topic0, pad(from), pad(to), token],
            data: Bytes::new(),
        };
        assert_eq!(
            transferred_token_id_from_logs(&[log(seller, relayer)], collection, seller, relayer),
            Some(U256::from(283u64))
        );
        assert_eq!(
            transferred_token_id_from_logs(&[log(other, relayer)], collection, seller, relayer),
            None,
            "a transfer from someone else is not this trade's delivery"
        );
        assert_eq!(
            transferred_token_id_from_logs(&[log(seller, other)], collection, seller, relayer),
            None,
            "a transfer to someone else must not be claimed"
        );
        // item orders mint: from = zero address
        assert_eq!(
            transferred_token_id_from_logs(
                &[log(Address::ZERO, relayer)],
                collection,
                Address::ZERO,
                relayer
            ),
            Some(U256::from(283u64))
        );
    }
}
