mod support;

const COLLECTION: &str = "0x7ad72b9f944ea9793cf4055d88f81138cc2c63a0";
const VICTIM: &str = "0x1111111111111111111111111111111111111111";
const ATTACKER: &str = "0x2222222222222222222222222222222222222222";

#[tokio::test]
async fn forged_from_cannot_burn_a_victims_relayer_quota() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let calldata = support::split_sig_calldata(ATTACKER);
    let (status, body) = support::post_transaction(&base, VICTIM, COLLECTION, &calldata).await;

    let victim_rows = support::row_count(&scratch.pool, VICTIM).await;
    let attacker_rows = support::row_count(&scratch.pool, ATTACKER).await;
    scratch.cleanup().await;

    assert_eq!(
        victim_rows, 0,
        "a meta-transaction signed by {ATTACKER} consumed a slot of {VICTIM}'s daily quota (status {status}, body {body})"
    );
    assert_eq!(
        attacker_rows, 0,
        "the forged request must not be relayed at all"
    );
    assert_eq!(
        status, 400,
        "a `from` that disagrees with the signed userAddress must be rejected, got body {body}"
    );
    assert_eq!(body["error"], "invalid_transaction", "body {body}");
}

#[tokio::test]
async fn quota_is_keyed_on_the_signed_user_address_for_both_overloads() {
    let Some(scratch) = support::setup_db().await else {
        return;
    };
    support::seed_collection(&scratch.pool, COLLECTION).await;
    let base = support::spawn_app(&scratch, 10).await;

    let split = support::split_sig_calldata(ATTACKER);
    let (split_status, split_body) =
        support::post_transaction(&base, ATTACKER, COLLECTION, &split).await;

    let combined = support::combined_sig_calldata(ATTACKER);
    let (combined_status, combined_body) =
        support::post_transaction(&base, ATTACKER, COLLECTION, &combined).await;

    let rows = support::row_count(&scratch.pool, ATTACKER).await;
    scratch.cleanup().await;

    assert_eq!(
        split_status, 200,
        "an honest split-sig meta-transaction must still relay, got {split_body}"
    );
    assert_eq!(
        combined_status, 200,
        "an honest combined-sig meta-transaction must still relay, got {combined_body}"
    );
    assert_eq!(rows, 2, "both relays are charged to the signing address");
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
