use std::env;

use alloy::primitives::keccak256;
use alloy::signers::{local::PrivateKeySigner, Signer};
use catalyrst_fed::Signed;
use catalyrst_market::fed::market_domain;
use catalyrst_market::fed::messages::OrderCreate;
use rand::Rng;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:5133".into());
    let item_id_arg = env::args()
        .nth(2)
        .unwrap_or_else(|| "0x002ed6d733c000ec8a23d720e06e229a1754a08d-0".into());
    let price_arg = env::args()
        .nth(3)
        .unwrap_or_else(|| "1000000000000000000".into());

    let root_hex = env::var("LANDILER_ROOT_KEY")
        .expect("set LANDILER_ROOT_KEY=<64 hex chars> (the stable root private key)");
    let root_bytes = hex::decode(root_hex.trim().trim_start_matches("0x"))
        .expect("LANDILER_ROOT_KEY must be hex");
    let wallet: PrivateKeySigner = PrivateKeySigner::from_slice(&root_bytes)?;
    let addr = format!("{:#x}", wallet.address());

    let eph_bytes = keccak256(&root_bytes);
    let ephemeral: PrivateKeySigner = PrivateKeySigner::from_slice(eph_bytes.as_slice())?;
    let ephemeral_addr = format!("{:#x}", ephemeral.address());

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = wallet.sign_message(ephemeral_payload.as_bytes()).await?;
    let ephemeral_sig_hex = ephemeral_sig.to_string();

    let ts_ms = chrono::Utc::now().timestamp_millis();
    let path = "/v1/federation/order";
    let metadata = "{}";
    let canonical = format!("post:{}:{}:{}", path, ts_ms, metadata).to_lowercase();
    let entity_sig = ephemeral.sign_message(canonical.as_bytes()).await?;
    let entity_sig_hex = entity_sig.to_string();

    let mut nonce = [0u8; 16];
    rand::rng().fill_bytes(&mut nonce);

    let now_s = ts_ms / 1000;
    let message = OrderCreate {
        item_id: item_id_arg.clone(),
        price: price_arg.clone(),
        expires_at: now_s + 24 * 60 * 60,
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
    println!("ADDR={}", addr);
    println!("ITEM_ID={}", item_id_arg);
    println!("status={} body={}", status, text);
    Ok(())
}
