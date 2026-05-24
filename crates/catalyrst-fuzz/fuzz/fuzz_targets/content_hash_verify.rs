#![no_main]
//! Fuzz `catalyrst_hashing::verify_hash`.
//!
//! `verify_hash` recently grew a strict CIDv0/CIDv1 shape whitelist
//! (`is_canonical_cid`); we want to confirm it never panics on adversarial
//! `expected` strings (path traversal, nul bytes, oversized inputs,
//! malformed multibase prefixes) and that `hash_bytes` / `hash_bytes_v1`
//! tolerate arbitrary byte slices.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        // Still exercise the empty-data path.
        let _ = catalyrst_hashing::hash_bytes(data);
        let _ = catalyrst_hashing::hash_bytes_v1(data);
        return;
    }
    // Use the first byte to pick a split point so the fuzzer can vary both
    // the "claimed CID string" and the payload bytes independently.
    let split = (data[0] as usize) % data.len();
    let (head, body) = data.split_at(split);
    let claimed = std::str::from_utf8(head).unwrap_or("");
    let _ = catalyrst_hashing::verify_hash(body, claimed);

    // Also drive the raw hash functions on the body so we cover their
    // (currently infallible) panic surface as well.
    let _ = catalyrst_hashing::hash_bytes(body);
    let _ = catalyrst_hashing::hash_bytes_v1(body);
});
