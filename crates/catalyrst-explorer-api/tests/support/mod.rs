#![allow(dead_code)]

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Request};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};

use catalyrst_crypto::signed_fetch::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_crypto::Wallet;

pub const SIGNER_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
pub const EPHEMERAL_KEY: &str =
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
pub const OTHER_KEY: &str = "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba";

pub const IDENTITY_PATH: &str = "/identities";

pub fn wallet(key: &str) -> Wallet {
    Wallet::from_hex(key).expect("test key")
}

fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn ephemeral_message(ephemeral_address: &str, expiration: DateTime<Utc>) -> String {
    format!(
        "Decentraland Login\nEphemeral address: {ephemeral_address}\nExpiration: {}",
        iso(expiration)
    )
}

pub fn mint_identity(signer_key: &str, ephemeral_key: &str, expiration: DateTime<Utc>) -> Value {
    let signer = wallet(signer_key);
    let ephemeral = wallet(ephemeral_key);
    let message = ephemeral_message(&ephemeral.address(), expiration);
    let signature = signer
        .sign_message(message.as_bytes())
        .expect("sign delegation");
    json!({
        "expiration": iso(expiration),
        "ephemeralIdentity": {
            "address": ephemeral.address(),
            "privateKey": ephemeral_key,
            "publicKey": "0x04-public-key-is-stored-verbatim",
        },
        "authChain": [
            { "type": "SIGNER", "payload": signer.address(), "signature": "" },
            { "type": "ECDSA_EPHEMERAL", "payload": message, "signature": signature },
        ],
    })
}

pub type Headers = Vec<(HeaderName, HeaderValue)>;

pub fn signed_fetch_headers(
    identity: &Value,
    ephemeral_key: &str,
    method: &str,
    path: &str,
    timestamp_ms: i64,
) -> Headers {
    let ephemeral = wallet(ephemeral_key);
    let payload = build_payload(method, path, &timestamp_ms.to_string(), "{}");
    let signature = ephemeral
        .sign_message(payload.as_bytes())
        .expect("sign request");
    let mut links: Vec<Value> = identity["authChain"]
        .as_array()
        .expect("identity chain")
        .clone();
    links.push(json!({
        "type": "ECDSA_SIGNED_ENTITY",
        "payload": payload,
        "signature": signature,
    }));
    let mut headers: Headers = links
        .iter()
        .enumerate()
        .map(|(i, link)| {
            (
                HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes())
                    .unwrap(),
                HeaderValue::from_str(&link.to_string()).unwrap(),
            )
        })
        .collect();
    headers.push((
        HeaderName::from_static(AUTH_TIMESTAMP_HEADER),
        HeaderValue::from_str(&timestamp_ms.to_string()).unwrap(),
    ));
    headers.push((
        HeaderName::from_static(AUTH_METADATA_HEADER),
        HeaderValue::from_static("{}"),
    ));
    headers
}

pub fn identity_post(identity: &Value, headers: Headers, real_ip: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/auth/identities")
        .header("content-type", "application/json")
        .header("x-real-ip", real_ip);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from(
            json!({ "identity": identity, "isMobile": false }).to_string(),
        ))
        .unwrap()
}

pub fn signed_identity_post(real_ip: &str) -> (Value, Request<Body>) {
    let identity = mint_identity(
        SIGNER_KEY,
        EPHEMERAL_KEY,
        Utc::now() + chrono::Duration::days(1),
    );
    let headers = signed_fetch_headers(
        &identity,
        EPHEMERAL_KEY,
        "post",
        IDENTITY_PATH,
        Utc::now().timestamp_millis(),
    );
    let request = identity_post(&identity, headers, real_ip);
    (identity, request)
}
