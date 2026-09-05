#![no_main]


use axum::http::{HeaderMap, HeaderName, HeaderValue};
use libfuzzer_sys::fuzz_target;

use catalyrst_builder::auth_chain::{extract_auth_chain, require_signer};
use catalyrst_builder::handlers::curation::authorize_admin;

fn fields(data: &[u8], max: usize) -> Vec<&[u8]> {
    data.split(|b| *b == 0).take(max).collect()
}

fn put(headers: &mut HeaderMap, name: &'static str, raw: &[u8]) {
    if let Ok(v) = HeaderValue::from_bytes(raw) {
        headers.insert(HeaderName::from_static(name), v);
    }
}

fuzz_target!(|data: &[u8]| {
    let parts = fields(data, 8);
    let mut headers = HeaderMap::new();

    let chain_headers = [
        "x-identity-auth-chain-0",
        "x-identity-auth-chain-1",
        "x-identity-auth-chain-2",
        "x-identity-auth-chain-3",
    ];
    for (i, name) in chain_headers.iter().enumerate() {
        if let Some(raw) = parts.get(i) {
            put(&mut headers, name, raw);
        }
    }
    if let Some(ts) = parts.get(4) {
        put(&mut headers, "x-identity-timestamp", ts);
    }
    if let Some(md) = parts.get(5) {
        put(&mut headers, "x-identity-metadata", md);
    }
    if let Some(p) = parts.get(6) {
        put(&mut headers, "x-original-path", p);
    }
    if let Some(a) = parts.get(7) {
        put(&mut headers, "authorization", a);
    }

    let _ = extract_auth_chain(&headers);

    let _ = require_signer(&headers, "get", "/v1/collections/curation");

    let admins = ["0x37c7728d6f29fa22bb9e1f1aa389a61a52ffd157".to_string()];
    let via_signature = authorize_admin(None, &admins, &headers, "get", "/v1/collections/curation");
    assert!(
        via_signature.is_err(),
        "fuzzed headers must never authorize via the signed-fetch branch"
    );

    let _ = authorize_admin(
        Some("a-non-empty-admin-secret-token"),
        &admins,
        &headers,
        "get",
        "/v1/collections/curation",
    );
});
