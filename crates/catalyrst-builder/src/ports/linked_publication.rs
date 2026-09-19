use alloy::{
    primitives::{address, keccak256, Address, B256, U256},
    sol_types::SolValue,
};
use serde::{Deserialize, Serialize};

use crate::http::errors::ApiError;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct LinkedPublicationCheque {
    pub qty: u32,
    pub salt: String,
    pub signature: String,
}

impl LinkedPublicationCheque {
    pub fn verify(
        &self,
        owner: &str,
        third_party_id: &str,
        expected_qty: u32,
        expected_salt: &str,
    ) -> Result<(), ApiError> {
        let owner: Address = owner
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid collection owner"))?;
        if self.qty == 0 || self.qty != expected_qty {
            return Err(ApiError::bad_request(
                "Slot authorization quantity does not match the reviewed items",
            ));
        }
        let salt = parse_salt(&self.salt)?;
        if salt != parse_salt(expected_salt)? {
            return Err(ApiError::bad_request(
                "Slot authorization does not match this publication attempt",
            ));
        }
        let digest = cheque_digest(third_party_id, self.qty, salt)?;
        let recovered =
            catalyrst_crypto::recover::recover_address_from_digest(&digest.0, &self.signature)
                .map_err(|_| ApiError::bad_request("Invalid slot authorization signature"))?;
        if !recovered.eq_ignore_ascii_case(&format!("{owner:#x}")) {
            return Err(ApiError::forbidden(
                "Slot authorization was not signed by the collection owner",
            ));
        }
        Ok(())
    }
}

fn parse_salt(salt: &str) -> Result<B256, ApiError> {
    if salt.len() != 66 || !salt.starts_with("0x") {
        return Err(ApiError::bad_request("Publication nonce must be 32 bytes"));
    }
    salt.parse()
        .map_err(|_| ApiError::bad_request("Publication nonce must be 32 bytes"))
}

fn registry_domain() -> B256 {
    keccak256(
        (
            keccak256(
                "EIP712Domain(string name,string version,address verifyingContract,bytes32 salt)",
            ),
            keccak256("Decentraland Third Party Registry"),
            keccak256("1"),
            address!("1C436C1EFb4608dFfDC8bace99d2B03c314f3348"),
            B256::from(U256::from(137).to_be_bytes::<32>()),
        )
            .abi_encode(),
    )
}

fn cheque_digest(third_party_id: &str, qty: u32, salt: B256) -> Result<B256, ApiError> {
    let provider = third_party_id
        .strip_prefix("urn:decentraland:matic:collections-thirdparty:")
        .filter(|id| {
            !id.is_empty()
                && !id
                    .chars()
                    .any(|c| c.is_whitespace() || c == ':' || c == '|')
        });
    if third_party_id.len() > 256 || provider.is_none() {
        return Err(ApiError::bad_request("Invalid linked provider URN"));
    }
    let message = keccak256(
        (
            keccak256("ConsumeSlots(string thirdPartyId,uint256 qty,bytes32 salt)"),
            keccak256(third_party_id),
            U256::from(qty),
            salt,
        )
            .abi_encode(),
    );
    let mut bytes = Vec::with_capacity(66);
    bytes.extend_from_slice(&[0x19, 0x01]);
    bytes.extend_from_slice(registry_domain().as_slice());
    bytes.extend_from_slice(message.as_slice());
    Ok(keccak256(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "0x19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A";
    const PROVIDER: &str = "urn:decentraland:matic:collections-thirdparty:provider";
    fn cheque() -> LinkedPublicationCheque {
        LinkedPublicationCheque {
            qty: 2,
            salt: format!("0x{}", "ab".repeat(32)),
            signature: "0x32141b34fc2592055513ef900955b86fbe4b0a708d230f7ba988948ea67d5ecf4452643c3eb73d2ee5718be1e014a95531de3a06b7bab8112816bf598af41bd31b".into(),
        }
    }

    #[test]
    fn matches_foundation_typed_data_and_polygon_registry_domain() {
        let cheque = cheque();
        assert_eq!(
            format!("{:#x}", registry_domain()),
            "0x7c22cdd2fa6286b864b9d5b8b0b0ee6eaee6235d17b48eb1231839228755b0ab"
        );
        assert_eq!(
            format!(
                "{:#x}",
                cheque_digest(PROVIDER, 2, parse_salt(&cheque.salt).unwrap()).unwrap()
            ),
            "0x0ee8c55105ec0418bb2f56be192e7b3c8d814026705d6699076714a43a8a3932"
        );
        cheque.verify(OWNER, PROVIDER, 2, &cheque.salt).unwrap();
    }

    #[test]
    fn binds_authorization_to_owner_provider_quantity_and_nonce() {
        let valid = cheque();
        assert!(valid
            .verify(
                "0x2222222222222222222222222222222222222222",
                PROVIDER,
                2,
                &valid.salt
            )
            .is_err());
        assert!(valid
            .verify(
                OWNER,
                "urn:decentraland:matic:collections-thirdparty:other",
                2,
                &valid.salt
            )
            .is_err());
        assert!(valid.verify(OWNER, PROVIDER, 1, &valid.salt).is_err());
        assert!(valid
            .verify(OWNER, PROVIDER, 2, &format!("0x{}", "cd".repeat(32)))
            .is_err());
        let mut changed = valid.clone();
        changed.qty = 1;
        assert!(changed.verify(OWNER, PROVIDER, 1, &valid.salt).is_err());
        changed.qty = 0;
        assert!(changed.verify(OWNER, PROVIDER, 0, &valid.salt).is_err());
    }

    #[test]
    fn rejects_invalid_nonce_signature_and_provider_encodings() {
        let valid = cheque();
        for salt in [
            "0x12".into(),
            "ab".repeat(32),
            format!("0x{}", "zz".repeat(32)),
        ] {
            let mut changed = valid.clone();
            changed.salt = salt.clone();
            assert!(changed.verify(OWNER, PROVIDER, 2, &salt).is_err());
        }
        for signature in ["".into(), "0x1234".into(), format!("0x{}", "00".repeat(65))] {
            let mut changed = valid.clone();
            changed.signature = signature;
            assert!(changed.verify(OWNER, PROVIDER, 2, &valid.salt).is_err());
        }
        for provider in [
            "",
            "urn:decentraland:matic:collections-thirdparty:",
            "urn:decentraland:amoy:collections-thirdparty:provider",
            "urn:decentraland:matic:collections-thirdparty:p:collection",
        ] {
            assert!(valid.verify(OWNER, provider, 2, &valid.salt).is_err());
        }
        assert!(valid
            .verify("not an address", PROVIDER, 2, &valid.salt)
            .is_err());
    }
}
