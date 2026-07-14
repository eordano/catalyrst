use alloy_primitives::{Address, U256};
use catalyrst_crypto::eip712::{
    domain_separator, hash_array_of_structs, hash_dynamic, struct_hash, typed_data_digest,
    word_address, word_u256,
};

use crate::types::ContractRentalListing;

fn rentals_contract(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        1 => Some("0x3a1469499d0be105d4f77045ca403a5f6dc2f3f5"),
        5 => Some("0x92159c78f0f4523b9c60382bb888f30f10a46b3b"),
        11155111 => Some("0xe70db6319e9cee3f604909bdade58d1f5c1cf702"),
        _ => None,
    }
}

const DOMAIN_NAME: &str = "Rentals";
const DOMAIN_VERSION: &str = "1";
const LISTING_TYPE: &str = "Listing(address signer,address contractAddress,uint256 tokenId,uint256 expiration,uint256[3] indexes,uint256[] pricePerDay,uint256[] maxDays,uint256[] minDays,address target)";

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("There is no rentals contract for chain id {0}")]
    ContractNotFound(u64),
    #[error("invalid number in listing field: {0}")]
    InvalidNumber(String),
    #[error("invalid signature: {0}")]
    Invalid(String),
}

fn parse_u256(s: &str) -> Result<U256, SignatureError> {
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(SignatureError::InvalidNumber(s.to_string()));
    }
    U256::from_str_radix(s, 10).map_err(|_| SignatureError::InvalidNumber(s.to_string()))
}

fn parse_address(addr: &str) -> Result<Address, SignatureError> {
    addr.parse()
        .map_err(|_| SignatureError::InvalidNumber(format!("address {}", addr)))
}

fn hash_u256_array(values: &[String]) -> Result<[u8; 32], SignatureError> {
    let mut words = Vec::with_capacity(values.len());
    for v in values {
        words.push(word_u256(parse_u256(v)?));
    }
    Ok(hash_array_of_structs(&words))
}

fn hash_u256_fixed3(values: &[String]) -> Result<[u8; 32], SignatureError> {
    if values.len() != 3 {
        return Err(SignatureError::InvalidNumber(format!(
            "indexes must have 3 elements, got {}",
            values.len()
        )));
    }
    hash_u256_array(values)
}

fn hash_listing(listing: &ContractRentalListing) -> Result<[u8; 32], SignatureError> {
    Ok(struct_hash(
        hash_dynamic(LISTING_TYPE.as_bytes()),
        &[
            word_address(parse_address(&listing.signer)?),
            word_address(parse_address(&listing.contract_address)?),
            word_u256(parse_u256(&listing.token_id)?),
            word_u256(parse_u256(&listing.expiration)?),
            hash_u256_fixed3(&listing.indexes)?,
            hash_u256_array(&listing.price_per_day)?,
            hash_u256_array(&listing.max_days)?,
            hash_u256_array(&listing.min_days)?,
            word_address(parse_address(&listing.target)?),
        ],
    ))
}

fn listing_digest(
    listing: &ContractRentalListing,
    chain_id: u64,
) -> Result<[u8; 32], SignatureError> {
    let verifying_contract =
        rentals_contract(chain_id).ok_or(SignatureError::ContractNotFound(chain_id))?;
    let domain = domain_separator(
        DOMAIN_NAME,
        DOMAIN_VERSION,
        chain_id,
        parse_address(verifying_contract)?,
    );
    Ok(typed_data_digest(domain, hash_listing(listing)?))
}

pub fn has_valid_v(signature: &str) -> bool {
    let s = signature.strip_prefix("0x").unwrap_or(signature);
    if s.len() != 130 {
        return false;
    }
    match u8::from_str_radix(&s[128..130], 16) {
        Ok(v) => v == 27 || v == 28,
        Err(_) => false,
    }
}

pub fn verify_rentals_listing_signature(
    listing: &ContractRentalListing,
    chain_id: u64,
) -> Result<bool, SignatureError> {
    let digest = listing_digest(listing, chain_id)?;

    let sig_str = listing
        .signature
        .strip_prefix("0x")
        .unwrap_or(&listing.signature);
    let sig_bytes = hex::decode(sig_str).map_err(|e| SignatureError::Invalid(e.to_string()))?;
    if sig_bytes.len() != 65 {
        return Err(SignatureError::Invalid(format!(
            "expected 65 signature bytes, got {}",
            sig_bytes.len()
        )));
    }
    let recovered =
        match catalyrst_crypto::recover::recover_address_from_digest(&digest, &listing.signature) {
            Ok(a) => a,
            Err(_) => return Ok(false),
        };

    Ok(recovered.eq_ignore_ascii_case(&listing.signer) && has_valid_v(&listing.signature))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector() -> ContractRentalListing {
        ContractRentalListing {
            signer: "0x19e7e376e7c213b7e7e7e46cc70a5dd086daff2a".to_string(),
            contract_address: "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d".to_string(),
            token_id: "42".to_string(),
            expiration: "1893456000".to_string(),
            indexes: vec!["0".into(), "0".into(), "0".into()],
            price_per_day: vec!["1000000000000000000".into()],
            max_days: vec!["30".into()],
            min_days: vec!["1".into()],
            target: "0x0000000000000000000000000000000000000000".to_string(),
            signature: "0xab333677b5572585e44bc94d70e60eef3468f9c08c896f44deb920a00599be71531f4a7bbbcff3d06de00a0c399ed09eeb342fb80a2eb898e887568c54ae20071c".to_string(),
        }
    }

    #[test]
    fn digest_matches_captured_vector() {
        assert_eq!(
            hex::encode(listing_digest(&vector(), 1).unwrap()),
            "d6c00455cf6c7c140ed004ba43dcb049b121ea69c690043c69431815343cbc82"
        );
    }

    #[test]
    fn accepts_valid_eip712_listing_signature() {
        assert!(verify_rentals_listing_signature(&vector(), 1).unwrap());
    }

    #[test]
    fn rejects_wrong_signer() {
        let mut v = vector();
        v.signer = "0x0000000000000000000000000000000000000001".to_string();
        assert!(!verify_rentals_listing_signature(&v, 1).unwrap());
    }

    #[test]
    fn rejects_tampered_field() {
        let mut v = vector();
        v.token_id = "43".to_string();
        assert!(!verify_rentals_listing_signature(&v, 1).unwrap());
    }

    #[test]
    fn unknown_chain_has_no_contract() {
        assert!(verify_rentals_listing_signature(&vector(), 999).is_err());
    }

    #[test]
    fn rejects_underscore_separated_numbers() {
        let mut v = vector();
        v.token_id = "4_2".to_string();
        assert!(verify_rentals_listing_signature(&v, 1).is_err());
    }

    #[test]
    fn rejects_high_s_malleated_signature() {
        let mut v = vector();
        let mut raw = hex::decode(v.signature.trim_start_matches("0x")).unwrap();
        let n = U256::from_str_radix(
            "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141",
            16,
        )
        .unwrap();
        let s = U256::from_be_slice(&raw[32..64]);
        raw[32..64].copy_from_slice(&(n - s).to_be_bytes::<32>());
        raw[64] = match raw[64] {
            27 => 28,
            28 => 27,
            other => other ^ 1,
        };
        v.signature = format!("0x{}", hex::encode(&raw));
        assert!(!verify_rentals_listing_signature(&v, 1).unwrap());
    }

    #[test]
    fn rejects_noncanonical_v() {
        assert!(!has_valid_v(&format!("0x{}", "ab".repeat(64) + "00")));
        assert!(has_valid_v(&format!("0x{}", "ab".repeat(64) + "1b")));
        assert!(has_valid_v(&format!("0x{}", "ab".repeat(64) + "1c")));
    }
}
