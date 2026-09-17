use alloy_primitives::keccak256;
use catalyrst_crypto::eip712::{
    domain_separator_salted, hash_dynamic, struct_hash, typed_data_digest, word_address, word_u64,
};

use super::contracts::CouponContracts;
use crate::ports::trades::eip712::{
    hash_checks, hex_bytes, parse_address, SignatureError, CHECKS_TYPE,
};
use crate::ports::trades::TradeChecksInput;

/// Rate discount, in parts per million: 300_000 is 30% off. The only discount type the shop
/// signs. A flat discount applies its amount to every received asset and reverts when it exceeds
/// an item's price.
pub const DISCOUNT_TYPE_RATE: i64 = 1;

const COUPON_TYPE_HEAD: &str = "Coupon(Checks checks,address couponAddress,bytes data)";

/// `Coupon` first, then its referenced structs in the alphabetical order EIP-712 mandates. The
/// Checks and ExternalCheck strings are the marketplace's own, so the two type sets cannot drift.
fn coupon_type_hash() -> [u8; 32] {
    let mut encoded = String::with_capacity(COUPON_TYPE_HEAD.len() + CHECKS_TYPE.len());
    encoded.push_str(COUPON_TYPE_HEAD);
    encoded.push_str(CHECKS_TYPE);
    hash_dynamic(encoded.as_bytes())
}

/// `abi.encode(CollectionDiscountCouponData)`: the bytes the creator signs and the contract
/// decodes.
pub fn encode_coupon_data(discount_type: i64, discount_ppm: i64, root: [u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(96);
    data.extend_from_slice(&word_u64(discount_type.max(0) as u64));
    data.extend_from_slice(&word_u64(discount_ppm.max(0) as u64));
    data.extend_from_slice(&root);
    data
}

pub fn coupon_manager_domain(
    chain_id: i64,
    contracts: &CouponContracts,
) -> Result<[u8; 32], SignatureError> {
    Ok(domain_separator_salted(
        contracts.coupon_manager.name,
        contracts.coupon_manager.version,
        parse_address(contracts.coupon_manager.address)?,
        word_u64(chain_id.max(0) as u64),
    ))
}

pub fn coupon_struct_hash(
    checks: &TradeChecksInput,
    coupon_address: &str,
    data: &[u8],
) -> Result<[u8; 32], SignatureError> {
    Ok(struct_hash(
        coupon_type_hash(),
        &[
            hash_checks(checks)?,
            word_address(parse_address(coupon_address)?),
            hash_dynamic(data),
        ],
    ))
}

pub fn coupon_signing_hash(
    chain_id: i64,
    contracts: &CouponContracts,
    checks: &TradeChecksInput,
    coupon_address: &str,
    data: &[u8],
) -> Result<[u8; 32], SignatureError> {
    Ok(typed_data_digest(
        coupon_manager_domain(chain_id, contracts)?,
        coupon_struct_hash(checks, coupon_address, data)?,
    ))
}

/// Whether `signature` is `signer`'s EIP-712 signature of this coupon against one
/// CouponManager. The recovery helper already refuses a non-canonical high `s` and an invalid
/// `v`, which is the ECDSA malleability guard upstream gets from re-serializing the signature
/// before it hashes one.
pub fn verify_coupon_signature(
    chain_id: i64,
    contracts: &CouponContracts,
    checks: &TradeChecksInput,
    coupon_address: &str,
    data: &[u8],
    signature: &str,
    signer: &str,
) -> bool {
    let Ok(digest) = coupon_signing_hash(chain_id, contracts, checks, coupon_address, data) else {
        return false;
    };
    let Ok(recovered) = catalyrst_crypto::recover::recover_address_from_digest(&digest, signature)
    else {
        return false;
    };
    recovered.eq_ignore_ascii_case(signer)
}

/// Which of the chain's coupon deployments `signature` was made against, or None if none. The
/// EIP-712 domain names its verifying contract, so a coupon signed against one manager verifies
/// against that one alone, and the match says which marketplace can redeem it.
pub fn resolve_coupon_signature(
    chain_id: i64,
    candidates: &[CouponContracts],
    checks: &TradeChecksInput,
    coupon_address: &str,
    data: &[u8],
    signature: &str,
    signer: &str,
) -> Option<CouponContracts> {
    candidates
        .iter()
        .find(|contracts| {
            verify_coupon_signature(
                chain_id,
                contracts,
                checks,
                coupon_address,
                data,
                signature,
                signer,
            )
        })
        .copied()
}

/// The two slots a CouponManager can record a coupon's uses and cancellation under, one per
/// generation of the contract. Both are live, because a coupon belongs to whichever manager it
/// was signed against.
///
/// The managers paired with the newest marketplace key on the EIP-712 digest, so that a
/// re-encoded signature cannot present itself as a fresh coupon. The earlier ones key on the
/// signature bytes. Which is which is not worth a table here: a coupon is bound to a single
/// manager, so at most one of these slots can ever hold anything, and reading both always
/// answers.
pub fn digest_coupon_state_key(signer: &str, digest: [u8; 32]) -> Result<[u8; 32], SignatureError> {
    coupon_state_key(signer, digest)
}

/// See [`digest_coupon_state_key`]: keccak256(abi.encode(signer, keccak256(signature))). Not
/// keccak256(signature) alone -- that reads zero forever on every deployed manager.
pub fn legacy_coupon_state_key(signer: &str, signature: &str) -> Result<[u8; 32], SignatureError> {
    coupon_state_key(signer, hashed_signature_bytes(signature)?)
}

fn coupon_state_key(signer: &str, handle: [u8; 32]) -> Result<[u8; 32], SignatureError> {
    let mut encoded = [0u8; 64];
    encoded[..32].copy_from_slice(&word_address(parse_address(signer)?));
    encoded[32..].copy_from_slice(&handle);
    Ok(keccak256(encoded).0)
}

/// keccak256 over the signature BYTES, the way the contract and the shop both hash it -- not
/// over its hex spelling.
pub fn hashed_signature_bytes(signature: &str) -> Result<[u8; 32], SignatureError> {
    Ok(keccak256(hex_bytes(signature)?).0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::coupons::contracts::coupon_contracts;
    use crate::ports::coupons::merkle::{collections_root, to_hex32};
    use crate::ports::trades::{ETHEREUM_MAINNET, MATIC_AMOY, MATIC_MAINNET};
    use alloy::signers::local::PrivateKeySigner;
    use alloy::signers::SignerSync;
    use alloy_primitives::{B256, U256};

    const COLLECTIONS: [&str; 5] = [
        "0x4c09495cd2d4e3d3fa2808eb655d013de426157b",
        "0xb0d0d31910da4a14d4e05a9d51b6e9a99a85d676",
        "0x7079fda5934f9bdcdfbe9c84e286a04dadfeb9e4",
        "0x96054dc54939d3c632796dbce4884705ed7c8977",
        "0xf8a87150ca602dbeb2e748ad7c9c790d55d10528",
    ];

    fn collections() -> Vec<String> {
        COLLECTIONS.iter().map(|c| c.to_string()).collect()
    }

    pub(crate) fn checks() -> TradeChecksInput {
        serde_json::from_value(serde_json::json!({
            "uses": 10,
            "expiration": 1_800_000_000_000i64,
            "effective": 1_700_000_000_000i64,
            "salt": format!("0x{}", "11".repeat(32)),
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": format!("0x{}", "00".repeat(32)),
            "externalChecks": []
        }))
        .unwrap()
    }

    fn wallet(seed: u8) -> PrivateKeySigner {
        let mut key = [0u8; 32];
        key[0] = 1;
        key[31] = seed;
        PrivateKeySigner::from_bytes(&B256::from(key)).expect("test key")
    }

    fn sign(signer: &PrivateKeySigner, digest: [u8; 32]) -> String {
        let signature = signer.sign_hash_sync(&B256::from(digest)).expect("sign");
        format!("0x{}", hex::encode(signature.as_bytes()))
    }

    /// Pinned rather than recomputed: re-expressing the implementation would assert nothing. The
    /// second value is what keying on the signature hash alone would produce, which reads zero
    /// from every manager forever, so the pair also documents the mistake it guards against.
    #[test]
    fn the_legacy_state_key_scopes_the_signature_hash_by_signer() {
        let signer = "0x4c09495cd2d4e3d3fa2808eb655d013de426157b";
        let signature = format!("0x{}", "ab".repeat(65));
        assert_eq!(
            to_hex32(legacy_coupon_state_key(signer, &signature).unwrap()),
            "0x05184e621d5f7d814b6684349ce2a8f07be24de1fa5f124f2914788c049b2ca0"
        );
        assert_eq!(
            to_hex32(hashed_signature_bytes(&signature).unwrap()),
            "0x1090dbec48f7f57f241cd63982ccab202c65844d3a54ee797b1a6de433635179"
        );
    }

    /// Taken from a coupon really applied on Amoy: the CouponManager wrote that use under the
    /// digest key below, and the signature key alongside it stayed at zero. Pinning both is what
    /// keeps the pair honest, since a key derived correctly but from the wrong handle looks
    /// exactly as plausible.
    #[test]
    fn a_digest_keyed_manager_writes_the_slot_the_digest_derives() {
        let contracts = coupon_contracts(MATIC_AMOY)[0];
        assert_eq!(
            contracts.coupon_manager.address,
            "0x6c956587d9fe70032781edcdc626310648575382"
        );
        let signer = "0x747c6f502272129bf1ba872a1903045b837ee86c";
        let signature = "0x8dfe3fa6844f0cdf55b7612bffa73bf846b161323f2b05c108f0dac55bd7832871783596ff6286e9f6cfc48d174e800e477d1c84e9b81acd0bc9b5b74407db4a1b";
        let checks: TradeChecksInput = serde_json::from_value(serde_json::json!({
            "uses": 5,
            "expiration": 1_789_732_387_809i64,
            "effective": 1_789_473_194_471i64,
            "salt": "0x3ecca08a6516479e33cf16ba01b056125a1e2eff4c9777035c0a60d98ebf34d0",
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": "0x",
            "externalChecks": []
        }))
        .unwrap();
        let root = from_hex32("0x69f6a818d79fb8cc8ff81eda2ea5e280154f97013ae27ded14bb8a7e33d24c78");
        let data = encode_coupon_data(DISCOUNT_TYPE_RATE, 300_000, root);

        let digest = coupon_signing_hash(
            MATIC_AMOY,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
        )
        .unwrap();
        assert_eq!(
            to_hex32(digest),
            "0x2b001c39e76fd747c79a907071a815b4c72a9a15a1d6e303e9aba11c2fd8650d"
        );
        assert_eq!(
            to_hex32(digest_coupon_state_key(signer, digest).unwrap()),
            "0x3c6b8e7a72677c18bf9579179ba86c80f66a15470474be6c796ae52ae6af1bca"
        );
        assert_ne!(
            legacy_coupon_state_key(signer, signature).unwrap(),
            digest_coupon_state_key(signer, digest).unwrap()
        );
    }

    fn from_hex32(value: &str) -> [u8; 32] {
        let raw = hex::decode(value.trim_start_matches("0x")).expect("hex");
        raw.try_into().expect("32 bytes")
    }

    /// The two domains differ only in verifyingContract, so a coupon can never verify against
    /// both: the signature alone says which manager, and so which marketplace, redeems it.
    #[test]
    fn the_signature_picks_which_manager_a_coupon_belongs_to() {
        let candidates = coupon_contracts(MATIC_MAINNET);
        let checks = checks();
        let data = encode_coupon_data(
            DISCOUNT_TYPE_RATE,
            300_000,
            collections_root(&collections()).unwrap(),
        );
        let creator = wallet(31);
        for contracts in &candidates {
            let digest = coupon_signing_hash(
                MATIC_MAINNET,
                contracts,
                &checks,
                contracts.collection_discount_coupon,
                &data,
            )
            .unwrap();
            let signature = sign(&creator, digest);
            assert_eq!(
                resolve_coupon_signature(
                    MATIC_MAINNET,
                    &candidates,
                    &checks,
                    contracts.collection_discount_coupon,
                    &data,
                    &signature,
                    &creator.address().to_string()
                ),
                Some(*contracts)
            );
            assert!(
                resolve_coupon_signature(
                    MATIC_MAINNET,
                    &candidates,
                    &checks,
                    contracts.collection_discount_coupon,
                    &data,
                    &signature,
                    &wallet(32).address().to_string()
                )
                .is_none(),
                "a signature belonging to somebody else resolves nothing"
            );
        }
    }

    #[test]
    fn a_chain_without_deployments_resolves_nothing() {
        let checks = checks();
        let data = encode_coupon_data(DISCOUNT_TYPE_RATE, 300_000, [0u8; 32]);
        assert!(resolve_coupon_signature(
            ETHEREUM_MAINNET,
            &coupon_contracts(ETHEREUM_MAINNET),
            &checks,
            "0x4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9",
            &data,
            &format!("0x{}", "ab".repeat(65)),
            "0x4c09495cd2d4e3d3fa2808eb655d013de426157b"
        )
        .is_none());
    }

    #[test]
    fn the_coupon_data_encodes_as_the_contract_decodes_it() {
        let root = collections_root(&collections()).unwrap();
        let data = encode_coupon_data(DISCOUNT_TYPE_RATE, 300_000, root);
        assert_eq!(data.len(), 96);
        assert_eq!(
            u64::from_be_bytes(data[24..32].try_into().unwrap()),
            DISCOUNT_TYPE_RATE as u64
        );
        assert_eq!(
            u64::from_be_bytes(data[56..64].try_into().unwrap()),
            300_000
        );
        assert_eq!(&data[64..], &root[..]);
    }

    #[test]
    fn a_creator_signature_verifies_and_a_stranger_does_not() {
        let contracts = coupon_contracts(MATIC_MAINNET)[0];
        let data = encode_coupon_data(
            DISCOUNT_TYPE_RATE,
            300_000,
            collections_root(&collections()).unwrap(),
        );
        let checks = checks();
        let creator = wallet(21);
        let digest = coupon_signing_hash(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
        )
        .unwrap();
        let signature = sign(&creator, digest);

        assert!(verify_coupon_signature(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
            &signature,
            &creator.address().to_string()
        ));
        assert!(!verify_coupon_signature(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
            &signature,
            &wallet(22).address().to_string()
        ));
    }

    #[test]
    fn a_discount_changed_after_signing_no_longer_verifies() {
        let contracts = coupon_contracts(MATIC_MAINNET)[0];
        let root = collections_root(&collections()).unwrap();
        let checks = checks();
        let creator = wallet(23);
        let digest = coupon_signing_hash(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &encode_coupon_data(DISCOUNT_TYPE_RATE, 300_000, root),
        )
        .unwrap();
        let signature = sign(&creator, digest);

        assert!(!verify_coupon_signature(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &encode_coupon_data(DISCOUNT_TYPE_RATE, 900_000, root),
            &signature,
            &creator.address().to_string()
        ));
    }

    #[test]
    fn a_structurally_invalid_signature_answers_false_rather_than_panicking() {
        let contracts = coupon_contracts(MATIC_MAINNET)[0];
        let checks = checks();
        let data = encode_coupon_data(DISCOUNT_TYPE_RATE, 300_000, [0u8; 32]);
        for signature in ["0x1234", "", "0xzz", &format!("0x{}", "00".repeat(65))] {
            assert!(
                !verify_coupon_signature(
                    MATIC_MAINNET,
                    &contracts,
                    &checks,
                    contracts.collection_discount_coupon,
                    &data,
                    signature,
                    "0x4c09495cd2d4e3d3fa2808eb655d013de426157b"
                ),
                "{signature}"
            );
        }
    }

    /// A flipped `s` recovers the same signer on a permissive stack; here it must not, because
    /// the two spellings hash differently and would key two rows to one on-chain coupon.
    #[test]
    fn a_malleated_high_s_signature_is_refused() {
        let contracts = coupon_contracts(MATIC_MAINNET)[0];
        let checks = checks();
        let data = encode_coupon_data(DISCOUNT_TYPE_RATE, 300_000, [0u8; 32]);
        let creator = wallet(24);
        let digest = coupon_signing_hash(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
        )
        .unwrap();
        let signature = sign(&creator, digest);

        assert!(verify_coupon_signature(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
            &signature,
            &creator.address().to_string()
        ));
        assert!(!verify_coupon_signature(
            MATIC_MAINNET,
            &contracts,
            &checks,
            contracts.collection_discount_coupon,
            &data,
            &flip_s(&signature),
            &creator.address().to_string()
        ));
    }

    fn flip_s(signature: &str) -> String {
        const ORDER: &str = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";
        let raw = hex::decode(signature.trim_start_matches("0x")).unwrap();
        let s = U256::from_be_slice(&raw[32..64]);
        let flipped = U256::from_str_radix(ORDER, 16).unwrap() - s;
        let mut out = raw.clone();
        out[32..64].copy_from_slice(&flipped.to_be_bytes::<32>());
        out[64] = if raw[64] == 27 { 28 } else { 27 };
        format!("0x{}", hex::encode(out))
    }
}
