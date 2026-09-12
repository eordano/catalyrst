use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

use super::contracts::{
    offchain_marketplace_v2, offchain_marketplace_v3, offchain_marketplaces, OffChainMarketplace,
    MATIC_AMOY, MATIC_MAINNET,
};
use super::create::{TradeCreation, TradeCreationError};
use super::eip712::{resolve_signature, signing_hash, verify_signature, SignatureError};

fn signer() -> PrivateKeySigner {
    "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318"
        .parse()
        .unwrap()
}

fn trade_json(signer_address: &str, signature: &str) -> String {
    trade_json_on(signer_address, signature, MATIC_MAINNET)
}

fn trade_json_on(signer_address: &str, signature: &str, chain_id: i64) -> String {
    format!(
        r#"{{
          "signer": "{signer_address}",
          "signature": "{signature}",
          "type": "public_nft_order",
          "network": "MATIC",
          "chainId": {chain_id},
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
    signed_trade_on(
        MATIC_MAINNET,
        &offchain_marketplace_v2(MATIC_MAINNET).unwrap(),
    )
}

fn signed_trade_on(chain_id: i64, marketplace: &OffChainMarketplace) -> TradeCreation {
    let wallet = signer();
    let address = wallet.address().to_checksum(None);
    let mut trade = parse(&trade_json_on(&address, "0x00", chain_id));
    let hash = signing_hash(&trade, marketplace).unwrap();
    let sig = wallet.sign_hash_sync(&hash).unwrap();
    trade.signature = format!("0x{}", hex::encode(sig.as_bytes()));
    trade
}

#[test]
fn a_trade_signed_against_v3_resolves_to_v3_with_its_digest() {
    let v3 = offchain_marketplace_v3(MATIC_AMOY).unwrap();
    let trade = signed_trade_on(MATIC_AMOY, &v3);
    let matched = resolve_signature(&trade, &offchain_marketplaces(MATIC_AMOY)).unwrap();
    assert_eq!(matched.marketplace, v3);
    let digest = matched
        .cancellation_digest
        .expect("V3 keys cancellations on the digest");
    assert_eq!(
        digest,
        format!("0x{:x}", signing_hash(&trade, &v3).unwrap())
    );
    assert_eq!(digest.len(), 66);
    assert_eq!(digest, digest.to_lowercase());
}

#[test]
fn a_trade_signed_against_v2_on_a_v3_chain_resolves_to_v2_without_a_digest() {
    let v2 = offchain_marketplace_v2(MATIC_AMOY).unwrap();
    let trade = signed_trade_on(MATIC_AMOY, &v2);
    let matched = resolve_signature(&trade, &offchain_marketplaces(MATIC_AMOY)).unwrap();
    assert_eq!(matched.marketplace, v2);
    assert_eq!(matched.cancellation_digest, None);
}

#[test]
fn mainnet_resolves_against_v2_alone_and_records_no_digest() {
    let trade = signed_trade();
    let matched = resolve_signature(&trade, &offchain_marketplaces(MATIC_MAINNET)).unwrap();
    assert_eq!(
        matched.marketplace,
        offchain_marketplace_v2(MATIC_MAINNET).unwrap()
    );
    assert_eq!(matched.cancellation_digest, None);
}

#[test]
fn a_signature_verifying_against_no_version_reports_the_newest_mismatch() {
    let trade = signed_trade();
    let err = resolve_signature(&trade, &offchain_marketplaces(MATIC_AMOY)).unwrap_err();
    assert!(matches!(err, SignatureError::Mismatch { .. }), "{err:?}");
}

/// Upstream 4614fa8: r outside the curve order or a non-canonical high s pass the v-byte
/// guard and must read as an invalid signature, not a server fault.
#[test]
fn a_structurally_invalid_signature_is_invalid_not_a_server_error() {
    let mut trade = signed_trade();
    let mut raw = hex::decode(trade.signature.trim_start_matches("0x")).unwrap();
    raw[..32].fill(0xff);
    trade.signature = format!("0x{}", hex::encode(&raw));
    let err = resolve_signature(&trade, &offchain_marketplaces(MATIC_MAINNET)).unwrap_err();
    assert!(matches!(err, SignatureError::Malformed(_)), "{err:?}");

    let mut trade = signed_trade();
    let mut raw = hex::decode(trade.signature.trim_start_matches("0x")).unwrap();
    raw[32..64].fill(0xff);
    trade.signature = format!("0x{}", hex::encode(&raw));
    assert!(resolve_signature(&trade, &offchain_marketplaces(MATIC_MAINNET)).is_err());
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
fn a_high_s_malleated_signature_is_rejected() {
    use alloy::primitives::U256;

    let mut trade = signed_trade();
    let marketplace = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    assert_eq!(verify_signature(&trade, &marketplace), Ok(()));

    let mut raw = hex::decode(trade.signature.trim_start_matches("0x")).unwrap();
    let n = U256::from_str_radix(
        "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141",
        16,
    )
    .unwrap();
    let s = U256::from_be_slice(&raw[32..64]);
    raw[32..64].copy_from_slice(&(n - s).to_be_bytes::<32>());
    raw[64] = match raw[64] {
        27 => 28,
        28 => 27,
        other => other ^ 1,
    };
    trade.signature = format!("0x{}", hex::encode(&raw));

    assert!(matches!(
        verify_signature(&trade, &marketplace),
        Err(SignatureError::Malformed(_))
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

fn trade_json_with_external_checks(signer_address: &str) -> String {
    format!(
        r#"{{
          "signer": "{signer_address}",
          "signature": "0x00",
          "type": "public_item_order",
          "network": "MATIC",
          "chainId": 137,
          "checks": {{
            "uses": 2,
            "expiration": 4102444800123,
            "effective": 1733691659485,
            "salt": "0x988c1638e5",
            "contractSignatureIndex": 3,
            "signerSignatureIndex": 5,
            "allowedRoot": "0xabcdef",
            "externalChecks": [
              {{"contractAddress": "0x4444444444444444444444444444444444444444", "selector": "0xdeadbeef", "value": "0x01f4", "required": true}},
              {{"contractAddress": "0x5555555555555555555555555555555555555555", "selector": "0x12345678", "required": false}}
            ]
          }},
          "sent": [
            {{"assetType": 4, "contractAddress": "0x6666666666666666666666666666666666666666", "itemId": "7", "extra": "0xc0ffee"}}
          ],
          "received": [
            {{"assetType": 2, "contractAddress": "0x7777777777777777777777777777777777777777", "amount": "150000000000000000000"}}
          ]
        }}"#
    )
}

#[test]
fn the_signing_hash_matches_its_captured_vectors() {
    let address = signer().address().to_checksum(None);
    let polygon = offchain_marketplace_v2(MATIC_MAINNET).unwrap();
    let ethereum = offchain_marketplace_v2(super::contracts::ETHEREUM_MAINNET).unwrap();

    let plain = parse(&trade_json(&address, "0x00"));
    assert_eq!(
        hex::encode(signing_hash(&plain, &polygon).unwrap()),
        "dc0df04c7e569fc389d3ac0e83953c19bd3b67a0846ab91e7051028da2b2dba9"
    );
    assert_eq!(
        hex::encode(signing_hash(&plain, &ethereum).unwrap()),
        "049f24ed8e4ddc3de92359db8c1850463463a72fc109cbdcdbf193471992fc9f"
    );

    let checked = parse(&trade_json_with_external_checks(&address));
    assert_eq!(
        hex::encode(signing_hash(&checked, &polygon).unwrap()),
        "0822b5db5233d3d47802e19cb9c612f2e3716fe2e1793030871b2f3cd0e7b14f"
    );
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
