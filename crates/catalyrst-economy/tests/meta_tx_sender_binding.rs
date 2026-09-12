mod support;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;

const COLLECTION: &str = "0x7ad72b9f944ea9793cf4055d88f81138cc2c63a0";
const VICTIM: &str = "0x1111111111111111111111111111111111111111";
const FS: &[u8] = &[0xaa, 0xbb, 0xcc, 0xdd];

#[tokio::test]
async fn honestly_signed_meta_transactions_relay_for_both_overloads() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let key = PrivateKeySigner::random();
    let user = format!("{:#x}", key.address());

    let split = support::signed_split_calldata(&key, FS);
    let (split_status, split_body) =
        support::post_transaction(&base, &user, COLLECTION, &split).await;

    let combined = support::signed_combined_calldata(&key, FS);
    let (combined_status, combined_body) =
        support::post_transaction(&base, &user, COLLECTION, &combined).await;

    let rows = support::row_count(&scratch.pool, &user).await;
    scratch.cleanup().await;

    assert_eq!(
        split_status, 200,
        "an honestly-signed split-sig meta-transaction must relay, got {split_body}"
    );
    assert_eq!(
        combined_status, 200,
        "an honestly-signed combined-sig meta-transaction must relay, got {combined_body}"
    );
    assert_eq!(
        rows, 2,
        "both honest relays are charged to the signing address"
    );
}

#[tokio::test]
async fn a_garbage_signature_is_rejected_and_charges_no_one() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let calldata = support::split_sig_calldata(VICTIM);
    let (status, body) = support::post_transaction(&base, VICTIM, COLLECTION, &calldata).await;

    let rows = support::row_count(&scratch.pool, VICTIM).await;
    scratch.cleanup().await;

    assert_eq!(
        rows, 0,
        "a garbage-signed meta-transaction must consume no quota (status {status}, body {body})"
    );
    assert_eq!(status, 400, "body {body}");
    assert_eq!(body["error"], "invalid_transaction", "body {body}");
}

#[tokio::test]
async fn a_signature_from_another_key_cannot_burn_a_victims_quota() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let attacker = PrivateKeySigner::random();
    let victim: Address = VICTIM.parse().expect("victim address");
    let calldata = support::signed_split_calldata_as(victim, &attacker, FS);
    let (status, body) = support::post_transaction(&base, VICTIM, COLLECTION, &calldata).await;

    let victim_rows = support::row_count(&scratch.pool, VICTIM).await;
    let attacker_rows =
        support::row_count(&scratch.pool, &format!("{:#x}", attacker.address())).await;
    scratch.cleanup().await;

    assert_eq!(
        victim_rows, 0,
        "a signature by another key must never consume {VICTIM}'s quota (status {status}, body {body})"
    );
    assert_eq!(
        attacker_rows, 0,
        "the forged request must not be relayed at all"
    );
    assert_eq!(status, 400, "body {body}");
    assert_eq!(body["error"], "invalid_transaction", "body {body}");
}

#[tokio::test]
async fn a_from_that_disagrees_with_the_signed_user_address_is_rejected() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let attacker = PrivateKeySigner::random();
    let calldata = support::signed_split_calldata(&attacker, FS);
    let (status, body) = support::post_transaction(&base, VICTIM, COLLECTION, &calldata).await;

    let victim_rows = support::row_count(&scratch.pool, VICTIM).await;
    let attacker_rows =
        support::row_count(&scratch.pool, &format!("{:#x}", attacker.address())).await;
    scratch.cleanup().await;

    assert_eq!(
        victim_rows, 0,
        "a `from` that disagrees with the signed userAddress must charge no one (status {status}, body {body})"
    );
    assert_eq!(
        attacker_rows, 0,
        "the mismatched request must not be relayed"
    );
    assert_eq!(status, 400, "body {body}");
    assert_eq!(body["error"], "invalid_transaction", "body {body}");
}

#[tokio::test]
async fn undecodable_meta_tx_calldata_is_rejected() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let (status, body) =
        support::post_transaction(&base, VICTIM, COLLECTION, "0x0c53c51cffffffffffffffff").await;

    let victim_rows = support::row_count(&scratch.pool, VICTIM).await;
    scratch.cleanup().await;

    assert_eq!(
        victim_rows, 0,
        "calldata whose userAddress cannot be recovered must never charge an address (status {status}, body {body})"
    );
    assert_eq!(status, 400, "body {body}");
    assert_eq!(body["error"], "invalid_transaction", "body {body}");
}

#[tokio::test]
async fn a_target_without_a_verifiable_domain_is_refused() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, support::CHAIN_UNVERIFIABLE_CONTRACT).await;
    let base = support::spawn_app(&scratch, 10).await;

    let key = PrivateKeySigner::random();
    let user = format!("{:#x}", key.address());
    let calldata = support::signed_split_calldata(&key, FS);
    let (status, body) = support::post_transaction(
        &base,
        &user,
        support::CHAIN_UNVERIFIABLE_CONTRACT,
        &calldata,
    )
    .await;

    let rows = support::row_count(&scratch.pool, &user).await;
    scratch.cleanup().await;

    assert_eq!(
        rows, 0,
        "an unverifiable target relays nothing (status {status}, body {body})"
    );
    assert_eq!(status, 400, "body {body}");
    assert_eq!(body["error"], "invalid_contract_address", "body {body}");
}
