use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, validate_signature, AuthChain, AuthChainError, AuthLink,
    AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

pub fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, AuthChainError> {
    signed_fetch::verify_signed_fetch(headers, method, path, FIVE_MINUTES).map(|a| a.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_timestamp_rejected_even_with_colon_in_path() {
        let chain = AuthChain {
            links: vec![],
            signer: String::new(),
        };
        let payload = "get:/world/urn:decentraland:foo:1000000000000:{}";
        let stale_ts = "1000000000000";
        let now = 2_000_000_000_i64;
        let r = validate_signature(&chain, payload, stale_ts, FIVE_MINUTES, now);
        assert!(
            matches!(r, Err(AuthChainError::Expired { .. })),
            "stale ts must be Expired"
        );
    }
}
