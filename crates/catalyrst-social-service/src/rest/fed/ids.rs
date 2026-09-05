use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn community_id_hex(creator: &str, name: &str, nonce: &[u8; 16]) -> String {
    let mut h = Sha256::new();
    h.update(creator.to_ascii_lowercase().as_bytes());
    h.update([0u8]);
    h.update(name.as_bytes());
    h.update([0u8]);
    h.update(nonce);
    hex::encode(h.finalize())
}

pub fn community_uuid_from_hex(hex_id: &str) -> Uuid {
    let bytes = hex::decode(hex_id).unwrap_or_default();
    let mut arr = [0u8; 16];
    if bytes.len() >= 16 {
        arr.copy_from_slice(&bytes[..16]);
    } else {
        arr[..bytes.len()].copy_from_slice(&bytes);
    }
    arr[6] = (arr[6] & 0x0f) | 0x40;
    arr[8] = (arr[8] & 0x3f) | 0x80;
    Uuid::from_bytes(arr)
}

pub fn signature_hash_hex(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}
