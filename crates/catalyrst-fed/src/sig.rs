use crate::error::FedError;
use catalyrst_crypto::recover::recover_address;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_SKEW_FUTURE_SECS: i64 = 30;
pub const MAX_SKEW_PAST_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip712Domain {
    pub name: String,
    pub version: String,
    #[serde(rename = "chainId")]
    pub chain_id: u64,
    #[serde(rename = "verifyingContract")]
    pub verifying_contract: String,
}

pub trait TypedMessage: Serialize + Clone + std::fmt::Debug {
    const PRIMARY_TYPE: &'static str;
    fn encode_struct(&self) -> Vec<u8>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signed<T: TypedMessage> {
    pub domain: Eip712Domain,
    pub message: T,
    pub nonce: [u8; 16],
    pub signed_at: i64,
    pub signature: String,
}

impl<T: TypedMessage> Signed<T> {
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"\x19\x01");
        h.update(domain_separator(&self.domain));
        h.update(self.message.encode_struct());
        h.update(self.nonce);
        h.update(self.signed_at.to_be_bytes());
        h.finalize().into()
    }

    pub fn signer(&self) -> Result<String, FedError> {
        decode_signature(&self.signature)?;
        let hash = self.hash();
        recover_address(&hash, &self.signature)
            .map_err(|e| FedError::InvalidSignature(e.to_string()))
    }

    pub fn verify(&self, expected_signer: &str, now: i64) -> Result<(), FedError> {
        let skew = now - self.signed_at;
        if !(-MAX_SKEW_FUTURE_SECS..=MAX_SKEW_PAST_SECS).contains(&skew) {
            return Err(FedError::SkewedTimestamp {
                signed_at: self.signed_at,
                now,
                skew,
            });
        }
        let recovered = self.signer()?;
        if !recovered.eq_ignore_ascii_case(expected_signer) {
            return Err(FedError::SignerMismatch {
                expected: expected_signer.to_string(),
                recovered,
            });
        }
        Ok(())
    }
}

fn decode_signature(sig: &str) -> Result<Vec<u8>, FedError> {
    let s = sig.strip_prefix("0x").unwrap_or(sig);
    if s.len() != 130 {
        return Err(FedError::InvalidSignature(format!(
            "expected 132 hex chars (with 0x), got {}",
            sig.len()
        )));
    }
    hex::decode(s).map_err(|e| FedError::InvalidSignature(e.to_string()))
}

fn domain_separator(d: &Eip712Domain) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(d.name.as_bytes());
    h.update(d.version.as_bytes());
    h.update(d.chain_id.to_be_bytes());
    h.update(d.verifying_contract.as_bytes());
    h.finalize().into()
}

pub mod domains {
    use super::Eip712Domain;

    pub fn places() -> Eip712Domain {
        Eip712Domain {
            name: "DecentralandPlaces".into(),
            version: "1".into(),
            chain_id: 137,
            verifying_contract: "0x0000000000000000000000000000000000000000".into(),
        }
    }

    pub fn events() -> Eip712Domain {
        Eip712Domain {
            name: "DecentralandEvents".into(),
            version: "1".into(),
            chain_id: 137,
            verifying_contract: "0x0000000000000000000000000000000000000000".into(),
        }
    }

    pub fn communities() -> Eip712Domain {
        Eip712Domain {
            name: "DecentralandCommunities".into(),
            version: "1".into(),
            chain_id: 137,
            verifying_contract: "0x0000000000000000000000000000000000000000".into(),
        }
    }

    pub fn friends() -> Eip712Domain {
        Eip712Domain {
            name: "DecentralandFriends".into(),
            version: "1".into(),
            chain_id: 137,
            verifying_contract: "0x0000000000000000000000000000000000000000".into(),
        }
    }

    pub fn messaging() -> Eip712Domain {
        Eip712Domain {
            name: "DecentralandMessaging".into(),
            version: "1".into(),
            chain_id: 137,
            verifying_contract: "0x0000000000000000000000000000000000000000".into(),
        }
    }
}
