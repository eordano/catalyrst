use alloy_primitives::{keccak256, Address, Signature, B256, U256};
use std::str::FromStr;

use super::contracts::OffChainMarketplace;
use super::create::{TradeAssetInput, TradeChecksInput, TradeCreation};

const DOMAIN_TYPE: &str =
    "EIP712Domain(string name,string version,address verifyingContract,bytes32 salt)";

const TRADE_TYPE: &str = concat!(
    "Trade(Checks checks,AssetWithoutBeneficiary[] sent,Asset[] received)",
    "Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)",
    "AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)",
    "Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)",
    "ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)",
);

const EXTERNAL_CHECK_TYPE: &str =
    "ExternalCheck(address contractAddress,bytes4 selector,bytes value,bool required)";

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

fn left_padded_32(value: &str) -> Result<B256, SignatureError> {
    let raw = hex_bytes(value)?;
    if raw.len() > 32 {
        return Err(SignatureError::Malformed(format!(
            "value wider than 32 bytes: {value}"
        )));
    }
    let mut out = [0u8; 32];
    out[32 - raw.len()..].copy_from_slice(&raw);
    Ok(B256::from(out))
}

fn right_padded_32(value: &str) -> Result<B256, SignatureError> {
    let raw = hex_bytes(value)?;
    if raw.len() > 32 {
        return Err(SignatureError::Malformed(format!(
            "value wider than 32 bytes: {value}"
        )));
    }
    let mut out = [0u8; 32];
    out[..raw.len()].copy_from_slice(&raw);
    Ok(B256::from(out))
}

fn address_word(value: &str) -> Result<B256, SignatureError> {
    let addr = Address::from_str(value)
        .map_err(|e| SignatureError::Malformed(format!("bad address {value}: {e}")))?;
    Ok(addr.into_word())
}

fn uint_word(value: &str) -> Result<B256, SignatureError> {
    let parsed = U256::from_str(value)
        .map_err(|e| SignatureError::Malformed(format!("bad uint {value}: {e}")))?;
    Ok(B256::from(parsed.to_be_bytes::<32>()))
}

fn u64_word(value: u64) -> B256 {
    B256::from(U256::from(value).to_be_bytes::<32>())
}

fn bool_word(value: bool) -> B256 {
    u64_word(u64::from(value))
}

// Seconds, not milliseconds: the contract signs the expiry the wallet showed the
// user, and the wire format carries milliseconds.
fn to_seconds(ms: i64) -> u64 {
    (ms / 1000).max(0) as u64
}

fn hash_external_checks(checks: &TradeChecksInput) -> Result<B256, SignatureError> {
    let type_hash = keccak256(EXTERNAL_CHECK_TYPE.as_bytes());
    let mut encoded = Vec::new();
    for check in checks.external_checks.iter().flatten() {
        let mut fields = Vec::with_capacity(160);
        fields.extend_from_slice(type_hash.as_slice());
        fields.extend_from_slice(address_word(&check.contract_address)?.as_slice());
        fields.extend_from_slice(right_padded_32(&check.selector)?.as_slice());
        let value = check.value.clone().unwrap_or_else(|| "0x".to_string());
        fields.extend_from_slice(keccak256(hex_bytes(&value)?).as_slice());
        fields.extend_from_slice(bool_word(check.required).as_slice());
        encoded.extend_from_slice(keccak256(&fields).as_slice());
    }
    Ok(keccak256(&encoded))
}

fn hash_checks(checks: &TradeChecksInput) -> Result<B256, SignatureError> {
    let type_hash = keccak256(
        "Checks(uint256 uses,uint256 expiration,uint256 effective,bytes32 salt,uint256 contractSignatureIndex,uint256 signerSignatureIndex,bytes32 allowedRoot,ExternalCheck[] externalChecks)"
            .as_bytes(),
    );
    let mut fields = Vec::with_capacity(288);
    fields.extend_from_slice(type_hash.as_slice());
    fields.extend_from_slice(u64_word(checks.uses).as_slice());
    fields.extend_from_slice(u64_word(to_seconds(checks.expiration)).as_slice());
    fields.extend_from_slice(u64_word(to_seconds(checks.effective)).as_slice());
    fields.extend_from_slice(left_padded_32(&checks.salt)?.as_slice());
    fields.extend_from_slice(u64_word(checks.contract_signature_index).as_slice());
    fields.extend_from_slice(u64_word(checks.signer_signature_index).as_slice());
    fields.extend_from_slice(left_padded_32(&checks.allowed_root)?.as_slice());
    fields.extend_from_slice(hash_external_checks(checks)?.as_slice());
    Ok(keccak256(&fields))
}

fn hash_assets(assets: &[TradeAssetInput], with_beneficiary: bool) -> Result<B256, SignatureError> {
    let type_hash = if with_beneficiary {
        keccak256(
            "Asset(uint256 assetType,address contractAddress,uint256 value,bytes extra,address beneficiary)"
                .as_bytes(),
        )
    } else {
        keccak256(
            "AssetWithoutBeneficiary(uint256 assetType,address contractAddress,uint256 value,bytes extra)"
                .as_bytes(),
        )
    };
    let mut encoded = Vec::new();
    for asset in assets {
        let mut fields = Vec::with_capacity(160);
        fields.extend_from_slice(type_hash.as_slice());
        fields.extend_from_slice(u64_word(asset.asset_type as u64).as_slice());
        fields.extend_from_slice(address_word(&asset.contract_address)?.as_slice());
        fields.extend_from_slice(uint_word(&asset.signed_value()?)?.as_slice());
        let extra = asset.extra.clone().unwrap_or_else(|| "0x".to_string());
        fields.extend_from_slice(keccak256(hex_bytes(&extra)?).as_slice());
        if with_beneficiary {
            let beneficiary = asset
                .beneficiary
                .clone()
                .unwrap_or_else(|| format!("{:?}", Address::ZERO));
            fields.extend_from_slice(address_word(&beneficiary)?.as_slice());
        }
        encoded.extend_from_slice(keccak256(&fields).as_slice());
    }
    Ok(keccak256(&encoded))
}

fn domain_separator(
    marketplace: &OffChainMarketplace,
    chain_id: i64,
) -> Result<B256, SignatureError> {
    let mut fields = Vec::with_capacity(160);
    fields.extend_from_slice(keccak256(DOMAIN_TYPE.as_bytes()).as_slice());
    fields.extend_from_slice(keccak256(marketplace.name.as_bytes()).as_slice());
    fields.extend_from_slice(keccak256(marketplace.version.as_bytes()).as_slice());
    fields.extend_from_slice(address_word(marketplace.address)?.as_slice());
    fields.extend_from_slice(u64_word(chain_id.max(0) as u64).as_slice());
    Ok(keccak256(&fields))
}

pub fn trade_struct_hash(trade: &TradeCreation) -> Result<B256, SignatureError> {
    let mut fields = Vec::with_capacity(128);
    fields.extend_from_slice(keccak256(TRADE_TYPE.as_bytes()).as_slice());
    fields.extend_from_slice(hash_checks(&trade.checks)?.as_slice());
    fields.extend_from_slice(hash_assets(&trade.sent, false)?.as_slice());
    fields.extend_from_slice(hash_assets(&trade.received, true)?.as_slice());
    Ok(keccak256(&fields))
}

pub fn signing_hash(
    trade: &TradeCreation,
    marketplace: &OffChainMarketplace,
) -> Result<B256, SignatureError> {
    let mut payload = Vec::with_capacity(66);
    payload.extend_from_slice(&[0x19, 0x01]);
    payload.extend_from_slice(domain_separator(marketplace, trade.chain_id)?.as_slice());
    payload.extend_from_slice(trade_struct_hash(trade)?.as_slice());
    Ok(keccak256(&payload))
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
    // Wallets emit v as 27/28; Signature::from_raw wants the 0/1 parity.
    let v = match raw[64] {
        27 | 28 => raw[64] - 27,
        0 | 1 => raw[64],
        other => {
            return Err(SignatureError::Malformed(format!(
                "unsupported signature v byte {other}"
            )))
        }
    };
    let mut normalised = raw.clone();
    normalised[64] = v;
    let signature = Signature::from_raw(&normalised)
        .map_err(|e| SignatureError::Malformed(format!("undecodable signature: {e}")))?;
    let hash = signing_hash(trade, marketplace)?;
    signature
        .recover_address_from_prehash(&hash)
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
