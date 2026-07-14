use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

use super::contracts::{offchain_marketplace_v2, MATIC_MAINNET};
use super::create::{TradeCreation, TradeCreationError};
use super::eip712::{signing_hash, verify_signature, SignatureError};

fn signer() -> PrivateKeySigner {
    "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318"
        .parse()
        .unwrap()
}

fn trade_json(signer_address: &str, signature: &str) -> String {
    format!(
        r#"{{
          "signer": "{signer_address}",
          "signature": "{signature}",
          "type": "public_nft_order",
          "network": "MATIC",
          "chainId": 137,
          "checks": {{
            "uses": 1,
            "expiration": 4102444800000,
            "effective": 0,
            "salt": "0x1234",
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": "0x",
            "externalChecks": []
          }},
          "sent": [
            {{"assetType": 3, "contractAddress": "0x1111111111111111111111111111111111111111", "tokenId": "42", "extra": "0x"}}
          ],
          "received": [
            {{"assetType": 1, "contractAddress": "0x2222222222222222222222222222222222222222", "amount": "1000", "extra": "0x", "beneficiary": "0x3333333333333333333333333333333333333333"}}
          ]
        }}"#
    )
}

fn parse(json: &str) -> TradeCreation {
    serde_json::from_str(json).expect("trade parses")
}

fn signed_trade() -> TradeCreation {
    let wallet = signer();
    let address = wallet.address().to_checksum(None);
    let mut trade = parse(&trade_json(&address, "0x00"));
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    let hash = signing_hash(&trade, &marketplace).unwrap();
    let sig = wallet.sign_hash_sync(&hash).unwrap();
    trade.signature = format!("0x{}", hex::encode(sig.as_bytes()));
    trade
}

#[test]
fn a_signature_over_the_trade_recovers_to_its_signer() {
    let trade = signed_trade();
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    assert_eq!(verify_signature(&trade, &marketplace), Ok(()));
}

#[test]
fn tampering_with_an_asset_breaks_the_signature() {
    let mut trade = signed_trade();
    trade.sent[0].token_id = Some("43".to_string());
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    assert!(matches!(
        verify_signature(&trade, &marketplace),
        Err(SignatureError::Mismatch { .. })
    ));
}

#[test]
fn tampering_with_the_price_breaks_the_signature() {
    let mut trade = signed_trade();
    trade.received[0].amount = Some("1".to_string());
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    assert!(matches!(
        verify_signature(&trade, &marketplace),
        Err(SignatureError::Mismatch { .. })
    ));
}

#[test]
fn a_signature_from_another_chain_does_not_verify() {
    let trade = signed_trade();
    let ethereum = offchain_marketplace_v2(super::contracts::ETHEREUM_MAINNET).unwrap();
    assert!(matches!(
        verify_signature(&trade, &ethereum),
        Err(SignatureError::Mismatch { .. })
    ));
}

#[test]
fn a_short_signature_is_rejected_before_recovery() {
    let mut trade = signed_trade();
    trade.signature = "0xdeadbeef".to_string();
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    assert!(matches!(
        verify_signature(&trade, &marketplace),
        Err(SignatureError::Malformed(_))
    ));
}

#[test]
fn the_signing_hash_is_stable() {
    let wallet = signer();
    let trade = parse(&trade_json(&wallet.address().to_checksum(None), "0x00"));
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    let first = signing_hash(&trade, &marketplace).unwrap();
    let second = signing_hash(&trade, &marketplace).unwrap();
    assert_eq!(first, second);
}

#[test]
fn the_zero_address_is_the_default_beneficiary() {
    let wallet = signer();
    let mut trade = parse(&trade_json(&wallet.address().to_checksum(None), "0x00"));
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    let with_explicit_zero = {
        trade.received[0].beneficiary = Some(Address::ZERO.to_checksum(None));
        signing_hash(&trade, &marketplace).unwrap()
    };
    let with_none = {
        trade.received[0].beneficiary = None;
        signing_hash(&trade, &marketplace).unwrap()
    };
    assert_eq!(with_explicit_zero, with_none);
}

#[tokio::test]
async fn an_expired_trade_is_refused_before_any_signature_work() {
    let Ok(url) = std::env::var("CATALYRST_MARKET_TEST_PG") else {
        eprintln!("skipping: CATALYRST_MARKET_TEST_PG unset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pg connects");
    let mut trade = signed_trade();
    trade.checks.expiration = 1_000;
    let err = super::create::create_trade(&pool, &trade, &trade.signer.clone(), 2_000, None)
        .await
        .unwrap_err();
    assert!(matches!(err, TradeCreationError::Expired));
}

#[tokio::test]
async fn an_erc721_listing_is_refused_when_ownership_cannot_be_checked() {
    let Ok(url) = std::env::var("CATALYRST_MARKET_TEST_PG") else {
        eprintln!("skipping: CATALYRST_MARKET_TEST_PG unset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pg connects");
    let trade = signed_trade();
    let err = super::create::create_trade(&pool, &trade, &trade.signer.clone(), 1_000, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, TradeCreationError::OwnershipUnverifiable(137)),
        "an unverifiable erc721 listing must fail closed, got {err:?}"
    );
}

#[tokio::test]
async fn a_bid_does_not_need_an_ownership_check() {
    let Ok(url) = std::env::var("CATALYRST_MARKET_TEST_PG") else {
        eprintln!("skipping: CATALYRST_MARKET_TEST_PG unset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pg connects");
    let mut trade = signed_trade();
    trade.trade_type = "bid".to_string();
    trade.checks.expiration = 1_000;
    let err = super::create::create_trade(&pool, &trade, &trade.signer.clone(), 2_000, None)
        .await
        .unwrap_err();
    assert!(matches!(err, TradeCreationError::Expired));
}

#[tokio::test]
async fn a_trade_signed_by_someone_else_is_refused() {
    let Ok(url) = std::env::var("CATALYRST_MARKET_TEST_PG") else {
        eprintln!("skipping: CATALYRST_MARKET_TEST_PG unset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pg connects");
    let trade = signed_trade();
    let err = super::create::create_trade(
        &pool,
        &trade,
        "0x9999999999999999999999999999999999999999",
        1_000,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TradeCreationError::SignerMismatch));
}
