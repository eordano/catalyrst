use ethers_core::types::{RecoveryMessage, Signature, H160};

use crate::AuthError;

pub fn recover_address(message: &[u8], signature: &str) -> Result<String, AuthError> {
    let sig_bytes = parse_signature_hex(signature)?;
    let sig = parse_ethers_signature(&sig_bytes)?;

    let recovered: H160 = sig
        .recover(RecoveryMessage::Data(message.to_vec()))
        .map_err(|e| AuthError::RecoveryFailed(format!("ecrecover failed: {}", e)))?;

    Ok(format!("{:#x}", recovered))
}

fn parse_signature_hex(hex: &str) -> Result<[u8; 65], AuthError> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);

    if hex.len() != 130 {
        return Err(AuthError::RecoveryFailed(format!(
            "Signature hex must be 130 characters (65 bytes), got {}",
            hex.len()
        )));
    }

    let bytes = hex::decode(hex).map_err(|e| {
        AuthError::RecoveryFailed(format!("Invalid hex in signature: {}", e))
    })?;

    let mut arr = [0u8; 65];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn parse_ethers_signature(bytes: &[u8; 65]) -> Result<Signature, AuthError> {
    let mut v = bytes[64];
    if v >= 27 {
        v -= 27;
    }

    let mut sig_bytes = [0u8; 65];
    sig_bytes[..64].copy_from_slice(&bytes[..64]);
    sig_bytes[64] = v;

    Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| AuthError::RecoveryFailed(format!("Invalid signature bytes: {}", e)))
}

mod hex {
    use crate::AuthError;

    pub fn decode(hex: &str) -> Result<Vec<u8>, AuthError> {
        if !hex.len().is_multiple_of(2) {
            return Err(AuthError::RecoveryFailed(
                "Odd-length hex string".into(),
            ));
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&hex[i..i + 2], 16)
                    .map_err(|e| AuthError::RecoveryFailed(format!("Hex decode: {}", e)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_signature_hex_strips_0x() {
        let hex_str = format!("0x{}", "00".repeat(65));
        let result = parse_signature_hex(&hex_str);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), [0u8; 65]);
    }

    #[test]
    fn test_parse_signature_hex_wrong_length() {
        assert!(parse_signature_hex("0xdeadbeef").is_err());
    }

    #[test]
    fn test_v_normalization() {
        let mut bytes = [0u8; 65];
        bytes[64] = 27;
        let sig = parse_ethers_signature(&bytes);
        assert!(sig.is_err() || sig.unwrap().v == 0);
    }
}
