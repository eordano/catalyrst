pub fn signature_hash_hex(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}
