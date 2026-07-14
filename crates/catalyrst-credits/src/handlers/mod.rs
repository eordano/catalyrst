pub mod admin;
pub mod authorize;
pub mod captcha;
pub mod cart;
pub mod orders;
pub mod packs;
pub mod ping;
pub mod prices;
pub mod seasons;
pub mod stripe;
pub mod topup;
pub mod users;
pub mod wallet;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};

pub async fn signer_from(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<catalyrst_crypto::Signer, ApiError> {
    require_signer(headers, method, path)
        .await
        .map_err(ApiError::from)
}

static RANDOM_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn random_32_hex() -> String {
    let mut h = Sha256::new();
    h.update(std::process::id().to_le_bytes());
    h.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes(),
    );
    h.update(RANDOM_COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hex::encode(h.finalize())
}
