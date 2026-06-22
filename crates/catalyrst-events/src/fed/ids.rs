use sha2::{Digest, Sha256};

/// Deterministic schedule id for a create (no upstream id supplied). Derived
/// from signer + name + nonce so replays of the same signed create resolve to
/// the same row.
pub fn schedule_id(signer: &str, name: &str, nonce: &[u8; 16]) -> String {
    let mut h = Sha256::new();
    h.update(signer.to_ascii_lowercase().as_bytes());
    h.update([0u8]);
    h.update(name.as_bytes());
    h.update([0u8]);
    h.update(nonce);
    hex::encode(h.finalize())
}

pub fn signature_hash_hex(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_id_is_deterministic_and_signer_case_insensitive() {
        let nonce = [7u8; 16];
        let a = schedule_id("0xABC", "MVMF", &nonce);
        let b = schedule_id("0xabc", "MVMF", &nonce);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn schedule_id_varies_by_name_and_nonce() {
        let n1 = [1u8; 16];
        let n2 = [2u8; 16];
        assert_ne!(
            schedule_id("0x1", "A", &n1),
            schedule_id("0x1", "B", &n1)
        );
        assert_ne!(
            schedule_id("0x1", "A", &n1),
            schedule_id("0x1", "A", &n2)
        );
    }
}
