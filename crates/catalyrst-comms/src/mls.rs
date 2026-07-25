use openmls::prelude::{
    Ciphersuite, KeyPackageIn, MlsMessageBodyIn, MlsMessageIn, ProtocolMessage, ProtocolVersion,
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tls_codec::DeserializeBytes;

use sha2::{Digest, Sha256};

pub const PINNED_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub const PINNED_CIPHERSUITE_ID: u16 = 0x0001;

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("malformed MLS message: {0}")]
    Malformed(String),
    #[error("wrong MLS message type: expected {expected}, got {got}")]
    WrongType {
        expected: &'static str,
        got: &'static str,
    },
    #[error("unsupported ciphersuite {got:#06x}; this federation only supports {want:#06x} (MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519)")]
    UnsupportedCiphersuite { got: u16, want: u16 },
    #[error("key package failed validation: {0}")]
    InvalidKeyPackage(String),
}

fn body_kind(body: &MlsMessageBodyIn) -> &'static str {
    match body {
        MlsMessageBodyIn::PublicMessage(_) => "public_message",
        MlsMessageBodyIn::PrivateMessage(_) => "private_message",
        MlsMessageBodyIn::Welcome(_) => "welcome",
        MlsMessageBodyIn::GroupInfo(_) => "group_info",
        MlsMessageBodyIn::KeyPackage(_) => "key_package",
    }
}

pub fn content_hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub struct ParsedKeyPackage {
    pub credential_identity: Vec<u8>,
    pub ciphersuite_id: u16,
    pub ref_hash: String,
}

pub fn parse_key_package(bytes: &[u8]) -> Result<ParsedKeyPackage, MlsError> {
    let (msg, _rest) = MlsMessageIn::tls_deserialize_bytes(bytes)
        .map_err(|e| MlsError::Malformed(e.to_string()))?;
    let body = msg.extract();
    let kp_in: KeyPackageIn = match body {
        MlsMessageBodyIn::KeyPackage(kp) => kp,
        other => {
            return Err(MlsError::WrongType {
                expected: "key_package",
                got: body_kind(&other),
            })
        }
    };

    let cred = kp_in.unverified_credential();
    let credential_identity = cred.credential.serialized_content().to_vec();

    let provider = OpenMlsRustCrypto::default();
    let kp = kp_in
        .validate(provider.crypto(), ProtocolVersion::Mls10)
        .map_err(|e| MlsError::InvalidKeyPackage(format!("{e:?}")))?;

    let cs = kp.ciphersuite();
    let cs_id = u16::from(cs);
    if cs != PINNED_CIPHERSUITE {
        return Err(MlsError::UnsupportedCiphersuite {
            got: cs_id,
            want: PINNED_CIPHERSUITE_ID,
        });
    }

    Ok(ParsedKeyPackage {
        credential_identity,
        ciphersuite_id: cs_id,
        ref_hash: content_hash(bytes),
    })
}

pub struct MessageRouting {
    pub group_id_hex: String,
    pub epoch: u64,

    pub wire: &'static str,
}

pub fn parse_message_routing(bytes: &[u8]) -> Result<MessageRouting, MlsError> {
    let (msg, _rest) = MlsMessageIn::tls_deserialize_bytes(bytes)
        .map_err(|e| MlsError::Malformed(e.to_string()))?;
    let body = msg.extract();
    let kind = body_kind(&body);
    let proto: ProtocolMessage = match body {
        MlsMessageBodyIn::PrivateMessage(m) => ProtocolMessage::PrivateMessage(m),
        MlsMessageBodyIn::PublicMessage(m) => ProtocolMessage::PublicMessage(Box::new(m)),
        _ => {
            return Err(MlsError::WrongType {
                expected: "private_message|public_message",
                got: kind,
            })
        }
    };
    let wire = match &proto {
        ProtocolMessage::PrivateMessage(_) => "private_message",
        ProtocolMessage::PublicMessage(_) => "public_message",
    };
    Ok(MessageRouting {
        group_id_hex: hex::encode(proto.group_id().as_slice()),
        epoch: proto.epoch().as_u64(),
        wire,
    })
}

pub fn parse_commit_routing(bytes: &[u8]) -> Result<MessageRouting, MlsError> {
    parse_message_routing(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_ciphersuite_id_matches_enum() {
        assert_eq!(u16::from(PINNED_CIPHERSUITE), PINNED_CIPHERSUITE_ID);
        assert_eq!(PINNED_CIPHERSUITE_ID, 0x0001);
    }

    #[test]
    fn garbage_is_rejected_not_panicked() {
        assert!(parse_key_package(&[0u8, 1, 2, 3]).is_err());
        assert!(parse_message_routing(&[9u8; 16]).is_err());
    }

    #[test]
    fn content_hash_is_sha256_hex() {
        assert_eq!(
            content_hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
