#![no_main]

use libfuzzer_sys::fuzz_target;
use catalyrst_types::AuthChain;

fuzz_target!(|data: &[u8]| {
    if let Ok(chain) = serde_json::from_slice::<AuthChain>(data) {
        let _ = catalyrst_crypto::auth_chain::is_valid_auth_chain(&chain);
    }
});
