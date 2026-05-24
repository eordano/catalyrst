#![no_main]
//! Fuzz auth-chain JSON deserialization.
//!
//! `AuthChain = Vec<AuthLink>` is what every signed deployment ships in its
//! HTTP multipart and in every snapshot line, so a panic in the
//! serde-derived `Deserialize` impl (or in `is_valid_auth_chain` running on
//! whatever shape parsed successfully) would be remotely triggerable.

use libfuzzer_sys::fuzz_target;
use catalyrst_types::AuthChain;

fuzz_target!(|data: &[u8]| {
    if let Ok(chain) = serde_json::from_slice::<AuthChain>(data) {
        // Drive the downstream shape-check on whatever parsed, so the fuzzer
        // also explores its branches (empty / SIGNER-position / length cap).
        let _ = catalyrst_crypto::auth_chain::is_valid_auth_chain(&chain);
    }
});
