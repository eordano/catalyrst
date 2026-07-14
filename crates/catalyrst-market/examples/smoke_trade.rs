use std::env;

use alloy::primitives::keccak256;
use alloy::signers::{local::PrivateKeySigner, Signer, SignerSync};
use catalyrst_market::ports::trades::{offchain_marketplace_v2, signing_hash, TradeCreation};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:5133".into());
    let kind = env::args().nth(2).unwrap_or_else(|| "bid".into());

    let root_hex = env::var("SMOKE_TRADE_KEY")
        .expect("set SMOKE_TRADE_KEY=<64 hex chars>; use a throwaway key, never a funded one");
    let root_bytes =
        hex::decode(root_hex.trim().trim_start_matches("0x")).expect("SMOKE_TRADE_KEY must be hex");
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

    let nft_contract = env::var("SMOKE_TRADE_NFT")
        .unwrap_or_else(|_| "0x000000000000000000000000000000000000dead".into());
    let nft_token = env::var("SMOKE_TRADE_TOKEN").unwrap_or_else(|_| "1".into());
    let chain_id = 137i64;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let expiration = now_ms + 7 * 24 * 60 * 60 * 1000;
    let salt = format!("0x{}", hex::encode(keccak256(now_ms.to_be_bytes())));

    let (trade_type, sent, received) = match kind.as_str() {
        "bid" => (
            "bid",
            serde_json::json!([{
                "assetType": 1,
                "contractAddress": "0x0000000000000000000000000000000000001010",
                "amount": "1",
                "extra": "0x"
            }]),
            serde_json::json!([{
                "assetType": 3,
                "contractAddress": nft_contract,
                "tokenId": nft_token,
                "extra": "0x",
                "beneficiary": addr
            }]),
        ),
        "nft" => (
            "public_nft_order",
            serde_json::json!([{
                "assetType": 3,
                "contractAddress": nft_contract,
                "tokenId": nft_token,
                "extra": "0x"
            }]),
            serde_json::json!([{
                "assetType": 1,
                "contractAddress": "0x0000000000000000000000000000000000001010",
                "amount": "1",
                "extra": "0x",
                "beneficiary": addr
            }]),
        ),
        other => anyhow::bail!("unknown kind {other}; use bid or nft"),
    };

    let body = serde_json::json!({
        "signer": addr,
        "signature": "0x00",
        "type": trade_type,
        "network": "MATIC",
        "chainId": chain_id,
        "checks": {
            "uses": 1,
            "expiration": expiration,
            "effective": now_ms,
            "salt": salt,
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": "0x",
            "externalChecks": []
        },
        "sent": sent,
        "received": received
    });

    let mut trade: TradeCreation = serde_json::from_value(body.clone())?;
    let marketplace = offchain_marketplace_v2(chain_id).expect("polygon marketplace");
    let hash = signing_hash(&trade, &marketplace).expect("trade hashes");
    let signature = wallet.sign_hash_sync(&hash)?;
    let signature_hex = format!("0x{}", hex::encode(signature.as_bytes()));
    trade.signature = signature_hex.clone();

    let mut body = body;
    body["signature"] = serde_json::Value::String(signature_hex);

    let path = "/v1/trades";
    let metadata = r#"{"signer":"dcl:marketplace","intent":"dcl:create-trade"}"#;
    let ts_ms = chrono::Utc::now().timestamp_millis();
    let canonical = format!("post:{}:{}:{}", path, ts_ms, metadata).to_lowercase();
    let entity_sig = ephemeral.sign_message(canonical.as_bytes()).await?;

    let link0 = serde_json::json!({"type":"SIGNER","payload":addr,"signature":""}).to_string();
    let link1 = serde_json::json!({
        "type":"ECDSA_EPHEMERAL",
        "payload":ephemeral_payload,
        "signature":ephemeral_sig.to_string()
    })
    .to_string();
    let link2 = serde_json::json!({
        "type":"ECDSA_SIGNED_ENTITY",
        "payload":canonical,
        "signature":entity_sig.to_string()
    })
    .to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let resp = client
        .post(format!("{base}{path}"))
        .header("content-type", "application/json")
        .header("x-identity-auth-chain-0", link0)
        .header("x-identity-auth-chain-1", link1)
        .header("x-identity-auth-chain-2", link2)
        .header("x-identity-timestamp", ts_ms.to_string())
        .header("x-identity-metadata", metadata)
        .body(serde_json::to_vec(&body)?)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    println!("KIND={kind}");
    println!("SIGNER={addr}");
    println!("status={status} body={text}");
    Ok(())
}
