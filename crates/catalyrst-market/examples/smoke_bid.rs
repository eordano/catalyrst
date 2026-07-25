use std::env;

use alloy::signers::{local::PrivateKeySigner, Signer};
use catalyrst_fed::Signed;
use catalyrst_market::fed::market_domain;
use catalyrst_market::fed::messages::BidPlace;
use rand::Rng;
use serde_json::Value;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:5139".into());
    let item_id_arg = env::args()
        .nth(2)
        .unwrap_or_else(|| "0xdeadbeef0000000000000000000000000000beef-0".into());

    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    key[0] |= 1;
    let wallet: PrivateKeySigner = PrivateKeySigner::from_slice(&key)?;
    let addr = format!("{:#x}", wallet.address());

    let ephemeral: PrivateKeySigner = PrivateKeySigner::random();
    let ephemeral_addr = format!("{:#x}", ephemeral.address());

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = wallet.sign_message(ephemeral_payload.as_bytes()).await?;
    let ephemeral_sig_hex = ephemeral_sig.to_string();

    let ts_ms = chrono::Utc::now().timestamp_millis();
    let path = "/v1/federation/bid";
    let metadata = "{}";
    let canonical = format!("post:{}:{}:{}", path, ts_ms, metadata).to_lowercase();
    let entity_sig = ephemeral.sign_message(canonical.as_bytes()).await?;
    let entity_sig_hex = entity_sig.to_string();

    let mut nonce = [0u8; 16];
    rand::rng().fill_bytes(&mut nonce);

    let now_s = ts_ms / 1000;
    let message = BidPlace {
        item_id: item_id_arg.clone(),
        price: "1000000000000000000".into(),
        expires_at: now_s + 24 * 60 * 60,
        fingerprint: String::new(),
        signed_at: now_s,
    };

    let mut signed = Signed {
        domain: market_domain(),
        message,
        nonce,
        signed_at: now_s,
        signature: String::new(),
    };
    let hash = signed.hash();
    let inner = wallet.sign_message(&hash).await?;
    signed.signature = inner.to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let url = format!("{}{}", base, path);
    let body = serde_json::to_vec(&signed)?;

    let link0 = serde_json::json!({"type":"SIGNER","payload":addr,"signature":""}).to_string();
    let link1 = serde_json::json!({
        "type":"ECDSA_EPHEMERAL",
        "payload":ephemeral_payload,
        "signature":ephemeral_sig_hex
    })
    .to_string();
    let link2 = serde_json::json!({
        "type":"ECDSA_SIGNED_ENTITY",
        "payload":canonical,
        "signature":entity_sig_hex
    })
    .to_string();

    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("x-identity-auth-chain-0", link0)
        .header("x-identity-auth-chain-1", link1)
        .header("x-identity-auth-chain-2", link2)
        .header("x-identity-timestamp", ts_ms.to_string())
        .header("x-identity-metadata", metadata)
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    println!("status={} body={}", status, text);
    if !status.is_success() {
        anyhow::bail!("bid place failed");
    }
    let sig_hash = parsed
        .get("signature_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing signature_hash"))?;
    if sig_hash.len() != 64 {
        anyhow::bail!("signature_hash wrong length");
    }
    println!("ok signature_hash={} item_id={}", sig_hash, item_id_arg);
    Ok(())
}
