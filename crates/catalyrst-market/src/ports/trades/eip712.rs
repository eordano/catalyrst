use alloy_primitives::{Address, B256, U256};
use catalyrst_crypto::eip712::{
    domain_separator_salted, hash_array_of_structs, hash_dynamic, struct_hash, typed_data_digest,
    word_address, word_u256, word_u64,
};
use std::str::FromStr;

use super::contracts::OffChainMarketplace;
use super::create::{TradeAssetInput, TradeChecksInput, TradeCreation};

const TRADE_TYPE: &str = concat!(
    "Trade(Checks checks,AssetWithoutBeneficiary[] sent,Asset[] received)",
    "Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)",
    "AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)",
    "Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)",
    "ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)",
);

const EXTERNAL_CHECK_TYPE: &str =
    "ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)";

// The referenced `ExternalCheck` type is deliberately NOT appended here: the trade
// signatures already in `marketplace.trades` were produced over the bare `Checks`
// string, so appending it would stop every stored signature from recovering.
const CHECKS_TYPE: &str = "Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)";

const ASSET_TYPE: &str =
    "Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)";

const ASSET_WITHOUT_BENEFICIARY_TYPE: &str =
    "AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)";

#[derive(Debug, PartialEq, Eq)]
pub enum SignatureError {
    Malformed(String),
    Mismatch { recovered: String, expected: String },
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignatureError::Malformed(why) => write!(f, "invalid trade signature: {why}"),
            SignatureError::Mismatch {
                recovered,
                expected,
            } => write!(
                f,
                "trade signature recovers to {recovered}, not the signer {expected}"
            ),
        }
    }
}

fn hex_bytes(value: &str) -> Result<Vec<u8>, SignatureError> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    hex::decode(trimmed).map_err(|e| SignatureError::Malformed(format!("bad hex {value}: {e}")))
}

fn left_padded_32(value: &str) -> Result<[u8; 32], SignatureError> {
    let raw = hex_bytes(value)?;
    if raw.len() > 32 {
        return Err(SignatureError::Malformed(format!(
            "value wider than 32 bytes: {value}"
        )));
    }
    let mut out = [0u8; 32];
    out[32 - raw.len()..].copy_from_slice(&raw);
    Ok(out)
}

fn right_padded_32(value: &str) -> Result<[u8; 32], SignatureError> {
    let raw = hex_bytes(value)?;
    if raw.len() > 32 {
        return Err(SignatureError::Malformed(format!(
            "value wider than 32 bytes: {value}"
        )));
    }
    let mut out = [0u8; 32];
    out[..raw.len()].copy_from_slice(&raw);
    Ok(out)
}

fn parse_address(value: &str) -> Result<Address, SignatureError> {
    Address::from_str(value)
        .map_err(|e| SignatureError::Malformed(format!("bad address {value}: {e}")))
}

fn address_word(value: &str) -> Result<[u8; 32], SignatureError> {
    Ok(word_address(parse_address(value)?))
}

fn uint_word(value: &str) -> Result<[u8; 32], SignatureError> {
    let parsed = U256::from_str(value)
        .map_err(|e| SignatureError::Malformed(format!("bad uint {value}: {e}")))?;
    Ok(word_u256(parsed))
}

fn bool_word(value: bool) -> [u8; 32] {
    word_u64(u64::from(value))
}

// Seconds, not milliseconds: the contract signs the expiry the wallet showed the
// user, and the wire format carries milliseconds.
fn to_seconds(ms: i64) -> u64 {
    (ms / 1000).max(0) as u64
}

fn hash_external_checks(checks: &TradeChecksInput) -> Result<[u8; 32], SignatureError> {
    let type_hash = hash_dynamic(EXTERNAL_CHECK_TYPE.as_bytes());
    let mut members = Vec::new();
    for check in checks.external_checks.iter().flatten() {
        let contract_address = address_word(&check.contract_address)?;
        let selector = right_padded_32(&check.selector)?;
        let value = check.value.clone().unwrap_or_else(|| "0x".to_string());
        members.push(struct_hash(
            type_hash,
            &[
                contract_address,
                selector,
                hash_dynamic(&hex_bytes(&value)?),
                bool_word(check.required),
            ],
        ));
    }
    Ok(hash_array_of_structs(&members))
}

fn hash_checks(checks: &TradeChecksInput) -> Result<[u8; 32], SignatureError> {
    Ok(struct_hash(
        hash_dynamic(CHECKS_TYPE.as_bytes()),
        &[
            word_u64(checks.uses),
            word_u64(to_seconds(checks.expiration)),
            word_u64(to_seconds(checks.effective)),
            left_padded_32(&checks.salt)?,
            word_u64(checks.contract_signature_index),
            word_u64(checks.signer_signature_index),
            left_padded_32(&checks.allowed_root)?,
            hash_external_checks(checks)?,
        ],
    ))
}

fn hash_assets(
    assets: &[TradeAssetInput],
    with_beneficiary: bool,
) -> Result<[u8; 32], SignatureError> {
    let type_hash = hash_dynamic(if with_beneficiary {
        ASSET_TYPE.as_bytes()
    } else {
        ASSET_WITHOUT_BENEFICIARY_TYPE.as_bytes()
    });
    let mut members = Vec::with_capacity(assets.len());
    for asset in assets {
        let mut fields = vec![
            word_u64(asset.asset_type as u64),
            address_word(&asset.contract_address)?,
            uint_word(&asset.signed_value()?)?,
            hash_dynamic(&hex_bytes(asset.extra.as_deref().unwrap_or("0x"))?),
        ];
        if with_beneficiary {
            let beneficiary = asset
                .beneficiary
                .clone()
                .unwrap_or_else(|| format!("{:?}", Address::ZERO));
            fields.push(address_word(&beneficiary)?);
        }
        members.push(struct_hash(type_hash, &fields));
    }
    Ok(hash_array_of_structs(&members))
}

fn trade_domain_separator(
    marketplace: &OffChainMarketplace,
    chain_id: i64,
) -> Result<[u8; 32], SignatureError> {
    Ok(domain_separator_salted(
        marketplace.name,
        marketplace.version,
        parse_address(marketplace.address)?,
        word_u64(chain_id.max(0) as u64),
    ))
}

pub fn trade_struct_hash(trade: &TradeCreation) -> Result<B256, SignatureError> {
    let checks = hash_checks(&trade.checks)?;
    let sent = hash_assets(&trade.sent, false)?;
    let received = hash_assets(&trade.received, true)?;
    Ok(B256::from(struct_hash(
        hash_dynamic(TRADE_TYPE.as_bytes()),
        &[checks, sent, received],
    )))
}

pub fn signing_hash(
    trade: &TradeCreation,
    marketplace: &OffChainMarketplace,
) -> Result<B256, SignatureError> {
    let domain = trade_domain_separator(marketplace, trade.chain_id)?;
    Ok(B256::from(typed_data_digest(
        domain,
        trade_struct_hash(trade)?.0,
    )))
}

pub fn recover_signer(
    trade: &TradeCreation,
    marketplace: &OffChainMarketplace,
) -> Result<Address, SignatureError> {
    let raw = hex_bytes(&trade.signature)?;
    if raw.len() != 65 {
        return Err(SignatureError::Malformed(format!(
            "expected a 65-byte signature, got {}",
            raw.len()
        )));
    }
    match raw[64] {
        0 | 1 | 27 | 28 => {}
        other => {
            return Err(SignatureError::Malformed(format!(
                "unsupported signature v byte {other}"
            )))
        }
    }
    let hash = signing_hash(trade, marketplace)?;
    let recovered = catalyrst_crypto::recover::recover_address_from_digest(
        &hash.0,
        &format!("0x{}", hex::encode(&raw)),
    )
    .map_err(|e| SignatureError::Malformed(format!("unrecoverable signature: {e}")))?;
    Address::from_str(&recovered)
        .map_err(|e| SignatureError::Malformed(format!("unrecoverable signature: {e}")))
}

pub fn verify_signature(
    trade: &TradeCreation,
    marketplace: &OffChainMarketplace,
) -> Result<(), SignatureError> {
    let recovered = recover_signer(trade, marketplace)?;
    let expected = Address::from_str(&trade.signer)
        .map_err(|e| SignatureError::Malformed(format!("bad signer {}: {e}", trade.signer)))?;
    if recovered == expected {
        Ok(())
    } else {
        Err(SignatureError::Mismatch {
            recovered: recovered.to_checksum(None),
            expected: expected.to_checksum(None),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SignatureMatch {
    pub marketplace: OffChainMarketplace,
    /// The identifier the matched version keys cancellations on, or None when
    /// it keys them on keccak256(signature bytes), which the trade already
    /// stores as hashed_signature.
    pub cancellation_digest: Option<String>,
}

/// Tries `candidates` in order and reports which one the signature verifies
/// against. A structurally invalid signature (bad length, v byte, r or s
/// outside the curve order, a non-canonical high s) is Malformed on the first
/// candidate and never a server fault: the high-s case is exactly the
/// malleability V3 exists to fix, so it has to read as an invalid signature.
/// A signature that verifies against no candidate reports the mismatch of
/// the newest one.
pub fn resolve_signature(
    trade: &TradeCreation,
    candidates: &[OffChainMarketplace],
) -> Result<SignatureMatch, SignatureError> {
    let mut first_mismatch: Option<SignatureError> = None;
    for marketplace in candidates {
        match verify_signature(trade, marketplace) {
            Ok(()) => {
                let cancellation_digest = if marketplace.cancels_by_digest {
                    Some(format!("0x{:x}", signing_hash(trade, marketplace)?))
                } else {
                    None
                };
                return Ok(SignatureMatch {
                    marketplace: *marketplace,
                    cancellation_digest,
                });
            }
            Err(malformed @ SignatureError::Malformed(_)) => return Err(malformed),
            Err(mismatch) => {
                first_mismatch.get_or_insert(mismatch);
            }
        }
    }
    Err(first_mismatch.unwrap_or_else(|| {
        SignatureError::Malformed("no off-chain marketplace to verify against".to_string())
    }))
}
