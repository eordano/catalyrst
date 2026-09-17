use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use alloy_primitives::B256;
use serde_json::{json, Value};

use chrono::{TimeZone, Utc};

use super::component::{state_keys_of, to_coupon, validate_creation};
use super::contracts::{coupon_contracts, CouponMarketplace};
use super::errors::{
    discount_out_of_bounds, too_many_collections, CouponError, AT_LEAST_ONE_USE,
    EFFECTIVE_BEFORE_EXPIRY, EXPIRATION_IN_THE_FUTURE, NO_ALLOWED_ROOT, NO_COLLECTIONS,
    NO_EXTERNAL_CHECKS, ONLY_PERCENTAGE_DISCOUNTS, RUNS_TOO_LONG, SCHEDULED_TOO_FAR_AHEAD,
};
use super::merkle::{collections_root, to_hex32};
use super::signature::{
    coupon_signing_hash, digest_coupon_state_key, encode_coupon_data, DISCOUNT_TYPE_RATE,
};
use super::types::{CouponCreation, CouponStatus, DbCouponWithState, MAX_COUPON_DURATION_MS};
use crate::ports::trades::{MATIC_AMOY, MATIC_MAINNET};

const NOW: i64 = 1_800_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1000;
const COLLECTION: &str = "0x4c09495cd2d4e3d3fa2808eb655d013de426157b";
const OTHER_COLLECTION: &str = "0xb0d0d31910da4a14d4e05a9d51b6e9a99a85d676";

fn wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[0] = 1;
    key[31] = seed;
    PrivateKeySigner::from_bytes(&B256::from(key)).expect("test key")
}

fn body(signer: &str) -> Value {
    json!({
        "signer": signer,
        "chainId": MATIC_MAINNET,
        "network": "MATIC",
        "checks": {
            "uses": 10,
            "expiration": NOW + 7 * DAY,
            "effective": NOW,
            "salt": format!("0x{}", "11".repeat(32)),
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": format!("0x{}", "00".repeat(32)),
            "externalChecks": []
        },
        "couponAddress": coupon_contracts(MATIC_MAINNET)[0].collection_discount_coupon,
        "discountType": DISCOUNT_TYPE_RATE,
        "discount": 300_000,
        "collections": [COLLECTION],
        "signature": format!("0x{}", "00".repeat(65))
    })
}

/// A body whose signature actually verifies, so every assertion below is about the check it
/// names rather than about an unsigned payload falling over at the last gate.
fn signed(value: Value, seed: u8) -> (CouponCreation, String) {
    signed_against(value, seed, 0)
}

/// `manager` indexes the chain's deployments newest first, so 0 signs against the current
/// marketplace's manager and 1 against the previous one.
fn signed_against(mut value: Value, seed: u8, manager: usize) -> (CouponCreation, String) {
    let creator = wallet(seed);
    let address = creator.address().to_string().to_lowercase();
    value["signer"] = json!(address);
    let mut coupon: CouponCreation = serde_json::from_value(value.clone()).expect("body parses");

    let contracts = coupon_contracts(coupon.chain_id)[manager];
    let root = collections_root(&coupon.collections).unwrap_or([0u8; 32]);
    let data = encode_coupon_data(coupon.discount_type, coupon.discount, root);
    let digest = coupon_signing_hash(
        coupon.chain_id,
        &contracts,
        &coupon.checks,
        &coupon.coupon_address,
        &data,
    )
    .expect("a digest");
    let signature = creator
        .sign_hash_sync(&B256::from(digest))
        .expect("sign digest");
    coupon.signature = format!("0x{}", hex::encode(signature.as_bytes()));
    (coupon, address)
}

fn creation(value: Value) -> (CouponCreation, String) {
    signed(value, 41)
}

fn refusal(value: Value) -> CouponError {
    let (coupon, signer) = creation(value);
    validate_creation(&coupon, &signer, NOW).expect_err("must be refused")
}

#[test]
fn a_discount_spelled_with_a_zero_fraction_reaches_the_component() {
    let mut value = body("");
    value["discount"] = json!(300_000.0);
    value["discountType"] = json!(DISCOUNT_TYPE_RATE as f64);
    let (coupon, signer) = creation(value);
    assert_eq!(coupon.discount, 300_000);
    assert_eq!(coupon.discount_type, DISCOUNT_TYPE_RATE);
    validate_creation(&coupon, &signer, NOW).expect("accepted");
}

#[test]
fn a_well_formed_coupon_passes_every_offline_check() {
    let (coupon, signer) = creation(body(""));
    let validated = validate_creation(&coupon, &signer, NOW).expect("accepted");
    assert_eq!(validated.network, "MATIC");
    assert_eq!(validated.collections, vec![COLLECTION.to_string()]);
    assert_eq!(validated.signature, coupon.signature.to_lowercase());
    assert_eq!(validated.data.len(), 96);
}

#[test]
fn the_caller_must_be_the_coupon_signer() {
    let (coupon, _) = creation(body(""));
    let err = validate_creation(&coupon, &wallet(42).address().to_string(), NOW)
        .expect_err("a stranger cannot post someone else's coupon");
    assert!(matches!(err, CouponError::InvalidSigner));
    assert_eq!(err.to_string(), "Coupon and request signer do not match");
}

#[test]
fn a_chain_without_collections_has_no_coupons() {
    let mut value = body("");
    value["chainId"] = json!(1);
    let (mut coupon, signer) = creation(body(""));
    coupon.chain_id = 1;
    let err = validate_creation(&coupon, &signer, NOW).expect_err("ethereum has no coupons");
    assert!(matches!(err, CouponError::UnsupportedChain(1)));
    assert_eq!(err.to_string(), "Coupons are not available on chain 1");
}

#[test]
fn the_coupon_address_must_be_the_chains_collection_discount_coupon() {
    let mut value = body("");
    value["couponAddress"] = json!("0x0000000000000000000000000000000000000009");
    assert!(matches!(refusal(value), CouponError::InvalidAddress));
}

#[test]
fn the_network_label_must_match_the_chain() {
    let mut value = body("");
    value["network"] = json!("ETHEREUM");
    let err = refusal(value);
    assert!(matches!(err, CouponError::InvalidNetwork));
    assert_eq!(
        err.to_string(),
        "The coupon network does not match its chain"
    );
}

#[test]
fn amoy_resolves_its_own_pair_and_stays_on_matic() {
    let mut value = body("");
    value["chainId"] = json!(MATIC_AMOY);
    value["couponAddress"] = json!(coupon_contracts(MATIC_AMOY)[0].collection_discount_coupon);
    let (coupon, signer) = creation(value);
    let validated = validate_creation(&coupon, &signer, NOW).expect("amoy is a coupon chain");
    assert_eq!(validated.network, "MATIC");
    assert_eq!(
        validated.contracts.coupon_manager.address,
        "0x6c956587d9fe70032781edcdc626310648575382"
    );
}

#[test]
fn only_percentage_discounts_are_supported() {
    let mut value = body("");
    value["discountType"] = json!(2);
    let err = refusal(value);
    assert_eq!(err.to_string(), ONLY_PERCENTAGE_DISCOUNTS);
}

#[test]
fn the_discount_is_bounded_at_both_ends() {
    for ppm in [49_999, 700_001, 0, -1] {
        let mut value = body("");
        value["discount"] = json!(ppm);
        assert_eq!(
            refusal(value).to_string(),
            discount_out_of_bounds(),
            "{ppm}"
        );
    }
    for ppm in [50_000, 700_000] {
        let mut value = body("");
        value["discount"] = json!(ppm);
        let (coupon, signer) = creation(value);
        assert!(
            validate_creation(&coupon, &signer, NOW).is_ok(),
            "{ppm} is on the boundary and must be accepted"
        );
    }
}

#[test]
fn a_coupon_must_cover_between_one_and_fifty_collections() {
    let mut value = body("");
    value["collections"] = json!([]);
    assert_eq!(refusal(value).to_string(), NO_COLLECTIONS);

    let many: Vec<String> = (0..51).map(|i| format!("0x{:040x}", i + 1)).collect();
    let mut value = body("");
    value["collections"] = json!(many);
    assert_eq!(refusal(value).to_string(), too_many_collections());
}

#[test]
fn duplicates_and_casing_collapse_before_the_bound_is_applied() {
    let mut value = body("");
    value["collections"] = json!([COLLECTION, COLLECTION.to_uppercase().replace("0X", "0x")]);
    let (coupon, signer) = creation(value);
    let validated = validate_creation(&coupon, &signer, NOW).expect("one collection, twice");
    assert_eq!(validated.collections, vec![COLLECTION.to_string()]);
}

#[test]
fn a_collection_that_is_not_an_address_is_refused_as_a_collections_problem() {
    let mut value = body("");
    value["collections"] = json!(["not-an-address"]);
    assert!(matches!(refusal(value), CouponError::InvalidCollections(_)));
}

#[test]
fn a_coupon_must_allow_at_least_one_use() {
    let mut value = body("");
    value["checks"]["uses"] = json!(0);
    assert_eq!(refusal(value).to_string(), AT_LEAST_ONE_USE);
}

#[test]
fn a_coupon_must_expire_in_the_future() {
    for expiration in [NOW, NOW - 1] {
        let mut value = body("");
        value["checks"]["effective"] = json!(expiration - DAY);
        value["checks"]["expiration"] = json!(expiration);
        assert_eq!(
            refusal(value).to_string(),
            EXPIRATION_IN_THE_FUTURE,
            "{expiration}"
        );
    }
}

/// `>=`, matching the table's own CHECK (`effective_since < expires_at`) exactly. With `>` a
/// zero-length window passed here and failed in Postgres, which reaches the creator as a 500.
#[test]
fn a_zero_length_sale_window_is_refused_before_the_table_sees_it() {
    let mut value = body("");
    value["checks"]["effective"] = json!(NOW + DAY);
    value["checks"]["expiration"] = json!(NOW + DAY);
    assert_eq!(refusal(value).to_string(), EFFECTIVE_BEFORE_EXPIRY);

    let mut value = body("");
    value["checks"]["effective"] = json!(NOW + DAY + 1);
    value["checks"]["expiration"] = json!(NOW + DAY);
    assert_eq!(refusal(value).to_string(), EFFECTIVE_BEFORE_EXPIRY);

    let mut value = body("");
    value["checks"]["effective"] = json!(NOW + DAY - 1);
    value["checks"]["expiration"] = json!(NOW + DAY);
    let (coupon, signer) = creation(value);
    assert!(
        validate_creation(&coupon, &signer, NOW).is_ok(),
        "one millisecond of window is still a window"
    );
}

#[test]
fn a_sale_may_not_be_scheduled_more_than_thirty_days_ahead() {
    let mut value = body("");
    value["checks"]["effective"] = json!(NOW + MAX_COUPON_DURATION_MS + 1);
    value["checks"]["expiration"] = json!(NOW + MAX_COUPON_DURATION_MS + 2);
    assert_eq!(refusal(value).to_string(), SCHEDULED_TOO_FAR_AHEAD);

    let mut value = body("");
    value["checks"]["effective"] = json!(NOW + MAX_COUPON_DURATION_MS);
    value["checks"]["expiration"] = json!(NOW + MAX_COUPON_DURATION_MS + DAY);
    let (coupon, signer) = creation(value);
    assert!(
        validate_creation(&coupon, &signer, NOW).is_ok(),
        "exactly thirty days ahead is still allowed"
    );
}

#[test]
fn a_sale_may_run_for_at_most_thirty_days() {
    let mut value = body("");
    value["checks"]["effective"] = json!(NOW);
    value["checks"]["expiration"] = json!(NOW + MAX_COUPON_DURATION_MS + 1);
    assert_eq!(refusal(value).to_string(), RUNS_TOO_LONG);

    let mut value = body("");
    value["checks"]["effective"] = json!(NOW);
    value["checks"]["expiration"] = json!(NOW + MAX_COUPON_DURATION_MS);
    let (coupon, signer) = creation(value);
    assert!(validate_creation(&coupon, &signer, NOW).is_ok());
}

/// The duration is measured from whichever of `effective` and now comes later, so a sale that
/// started long ago is judged on what is left of it.
#[test]
fn a_sale_already_running_is_measured_from_now() {
    let mut value = body("");
    value["checks"]["effective"] = json!(NOW - 60 * DAY);
    value["checks"]["expiration"] = json!(NOW + DAY);
    let (coupon, signer) = creation(value);
    assert!(validate_creation(&coupon, &signer, NOW).is_ok());
}

#[test]
fn a_coupon_may_not_restrict_who_uses_it_or_carry_external_checks() {
    let mut value = body("");
    value["checks"]["allowedRoot"] = json!(format!("0x{}", "22".repeat(32)));
    assert_eq!(refusal(value).to_string(), NO_ALLOWED_ROOT);

    for root in ["0x", "", format!("0x{}", "00".repeat(32)).as_str()] {
        let mut value = body("");
        value["checks"]["allowedRoot"] = json!(root);
        let (coupon, signer) = creation(value);
        assert!(
            validate_creation(&coupon, &signer, NOW).is_ok(),
            "{root} spells no allow-list"
        );
    }

    let mut value = body("");
    value["checks"]["externalChecks"] = json!([{
        "contractAddress": COLLECTION,
        "selector": "0x12345678",
        "value": "0x",
        "required": true
    }]);
    assert_eq!(refusal(value).to_string(), NO_EXTERNAL_CHECKS);
}

#[test]
fn a_signature_of_the_wrong_length_or_v_byte_is_refused_before_recovery() {
    for signature in [
        format!("0x{}", "11".repeat(64)),
        format!("0x{}", "11".repeat(66)),
        format!("0x{}00", "11".repeat(64)),
        format!("0x{}1a", "11".repeat(64)),
    ] {
        let (mut coupon, signer) = creation(body(""));
        coupon.signature = signature.clone();
        let err = validate_creation(&coupon, &signer, NOW).expect_err(&signature);
        assert!(matches!(err, CouponError::InvalidSignature), "{signature}");
        assert_eq!(err.to_string(), "Invalid coupon signature");
    }
}

#[test]
fn a_signature_over_a_different_collection_set_no_longer_verifies() {
    let (mut coupon, signer) = creation(body(""));
    coupon.collections = vec![OTHER_COLLECTION.to_string()];
    let err = validate_creation(&coupon, &signer, NOW).expect_err("the root moved");
    assert!(matches!(err, CouponError::InvalidSignature));
}

#[test]
fn the_state_key_and_hashed_signature_come_from_the_signature_bytes() {
    let (coupon, signer) = creation(body(""));
    let validated = validate_creation(&coupon, &signer, NOW).unwrap();
    assert_ne!(validated.state_key, validated.hashed_signature);
    assert_eq!(
        validated.hashed_signature,
        super::signature::hashed_signature_bytes(&coupon.signature.to_lowercase()).unwrap()
    );
}

fn stored_row(checks: Value, uses: i32) -> DbCouponWithState {
    let checked_at = Utc.timestamp_millis_opt(NOW).unwrap();
    DbCouponWithState {
        id: "0f0a2f3c-0000-4000-8000-000000000001".to_string(),
        network: "MATIC".to_string(),
        chain_id: MATIC_MAINNET as i32,
        signer: COLLECTION.to_string(),
        signature: format!("0x{}1b", "11".repeat(64)),
        state_key: format!("0x{}", "22".repeat(32)),
        coupon_manager: COLLECTION.to_string(),
        coupon_address: COLLECTION.to_string(),
        checks,
        discount_type: DISCOUNT_TYPE_RATE as i16,
        discount_ppm: 300_000,
        root: format!("0x{}", "33".repeat(32)),
        collections: vec![COLLECTION.to_string()],
        effective_since: checked_at,
        expires_at: Utc.timestamp_millis_opt(NOW + 7 * DAY).unwrap(),
        created_at: checked_at,
        state_uses: Some(uses),
        state_cancelled: Some(false),
        state_revoked: Some(false),
        state_checked_at: Some(checked_at),
    }
}

#[test]
fn a_row_whose_stored_checks_cannot_be_read_is_not_reported_as_exhausted() {
    let unreadable = to_coupon(&stored_row(json!({}), 5), NOW + DAY);
    assert_eq!(unreadable.status, CouponStatus::Active);

    let readable = to_coupon(
        &stored_row(
            json!({"uses": 5, "contractSignatureIndex": 0, "signerSignatureIndex": 0}),
            5,
        ),
        NOW + DAY,
    );
    assert_eq!(readable.status, CouponStatus::Exhausted);
}

/// Both managers of a chain are live while clients move over, so a coupon signed against the
/// previous marketplace's manager is stored against that manager and reports it.
#[test]
fn a_coupon_signed_against_the_previous_manager_is_still_accepted() {
    let (coupon, signer) = signed_against(body(""), 44, 1);
    let validated = validate_creation(&coupon, &signer, NOW).expect("accepted");
    assert_eq!(
        validated.contracts.marketplace,
        CouponMarketplace::OffChainMarketplaceV2
    );
    assert_eq!(
        validated.contracts.coupon_manager.address,
        "0x3fd3056ee72a2a85e9392fab3a450e7736536081"
    );

    let (coupon, signer) = signed_against(body(""), 44, 0);
    let validated = validate_creation(&coupon, &signer, NOW).expect("accepted");
    assert_eq!(
        validated.contracts.marketplace,
        CouponMarketplace::OffChainMarketplaceV3
    );
    assert_eq!(
        validated.contracts.coupon_manager.address,
        "0x655fdfa91d69ea49f4ce1a8f7f7e2622c8630813"
    );
}

#[test]
fn a_stored_coupon_reports_the_marketplace_its_manager_is_wired_into() {
    let mut row = stored_row(json!({}), 0);
    row.coupon_manager = "0x3FD3056EE72A2A85E9392FAB3A450E7736536081".to_string();
    assert_eq!(
        to_coupon(&row, NOW).marketplace,
        Some(CouponMarketplace::OffChainMarketplaceV2)
    );

    row.coupon_manager = "0x655fdfa91d69ea49f4ce1a8f7f7e2622c8630813".to_string();
    assert_eq!(
        to_coupon(&row, NOW).marketplace,
        Some(CouponMarketplace::OffChainMarketplaceV3)
    );

    row.coupon_manager = format!("0x{}", "99".repeat(20));
    assert_eq!(to_coupon(&row, NOW).marketplace, None);
}

/// The stored slot is the one the signature-keyed managers write; the digest slot is rebuilt
/// from the row, so a coupon is read from whichever of the two its own manager uses.
#[test]
fn a_refresh_asks_for_both_slots_a_manager_generation_could_have_written() {
    let (coupon, signer) = creation(body(""));
    let validated = validate_creation(&coupon, &signer, NOW).expect("accepted");
    let mut row = stored_row(
        json!({
            "uses": 10,
            "expiration": NOW + 7 * DAY,
            "effective": NOW,
            "salt": format!("0x{}", "11".repeat(32)),
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": format!("0x{}", "00".repeat(32)),
            "externalChecks": []
        }),
        0,
    );
    row.signer = signer.clone();
    row.coupon_manager = validated.contracts.coupon_manager.address.to_string();
    row.coupon_address = coupon.coupon_address.clone();
    row.root = to_hex32(validated.root);
    row.state_key = to_hex32(validated.state_key);

    let digest = coupon_signing_hash(
        MATIC_MAINNET,
        &validated.contracts,
        &coupon.checks,
        &coupon.coupon_address,
        &validated.data,
    )
    .unwrap();
    assert_eq!(
        state_keys_of(&row),
        Some(vec![
            to_hex32(digest_coupon_state_key(&signer, digest).unwrap()),
            to_hex32(validated.state_key),
        ])
    );
}

#[test]
fn a_row_naming_a_manager_no_longer_deployed_has_no_slot_to_read() {
    let mut row = stored_row(json!({}), 0);
    row.coupon_manager = format!("0x{}", "99".repeat(20));
    assert!(state_keys_of(&row).is_none());
}
