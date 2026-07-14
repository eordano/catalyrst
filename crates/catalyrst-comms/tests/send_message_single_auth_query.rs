//! Perf regression: `send_message`'s pre-write phase (epoch fetch + membership
//! check) must be ONE SELECT, not two. Also pins the error precedence:
//! missing group -> 404 before any 403, non-member -> 403.
//!
//! The signed-fetch payload covers method/path/timestamp/metadata but NOT the
//! body, so a garbage body passes auth and membership and only dies at the
//! ciphertext parse (400). That exercises the whole pre-write DB phase without
//! constructing real MLS framing.
//!
//! DB-gated via the shared scratch cluster; skips cleanly when unset.
//! MUST be a current-thread `#[tokio::test]` -- see `support`'s SQL-counter docs.

mod support;

use axum::body::Bytes;
use axum::extract::{Path, State};
use catalyrst_comms::handlers::messaging::{create_group, send_message};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_crypto::Wallet;
use serde_json::json;

// Well-known hardhat account #1 / #2 private keys -- valid secp256k1 keys.
const CREATOR_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const OUTSIDER_KEY: &str = "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a";

fn fresh_group_id() -> String {
    format!("{:064x}", uuid::Uuid::new_v4().as_u128())
}

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_sendmsg").await?;
    scratch
        .apply_sql(include_str!("../migrations/0004_mls_messaging.sql"))
        .await;
    Some(scratch)
}

#[tokio::test]
async fn send_message_pre_write_phase_is_one_select() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let state = support::test_state(pool.clone(), None, None, "squid_marketplace");

    let creator = Wallet::from_hex(CREATOR_KEY).unwrap();
    let outsider = Wallet::from_hex(OUTSIDER_KEY).unwrap();
    let gid = fresh_group_id();

    let _ = create_group(
        State(state.clone()),
        support::signed_headers(&creator, "post", "/mls/groups"),
        Bytes::from(
            json!({
                "group_id": gid,
                "group_kind": "channel",
                "initial_members": [creator.address().to_lowercase()],
            })
            .to_string(),
        ),
    )
    .await
    .expect("group creation must succeed");

    let path = format!("/mls/groups/{gid}/messages");

    // Install AFTER seeding so create_group's statements are outside the window.
    let (counter, _guard) = support::install_sql_counter();

    // (a) member + garbage body: passes auth + membership, 400 at ciphertext parse.
    let err = send_message(
        State(state.clone()),
        support::signed_headers(&creator, "post", &path),
        Path(gid.clone()),
        Bytes::from_static(b"not json"),
    )
    .await
    .expect_err("garbage body must 400");
    assert_eq!(err.code, 400, "member+garbage body is a 400");
    assert_eq!(
        counter.count_containing("mls_group"),
        1,
        "auth+epoch phase must be exactly one SELECT"
    );

    // (b) non-member: 403, still one statement.
    counter.reset();
    let err = send_message(
        State(state.clone()),
        support::signed_headers(&outsider, "post", &path),
        Path(gid.clone()),
        Bytes::from_static(b"not json"),
    )
    .await
    .expect_err("non-member must be refused");
    assert_eq!(err.code, 403, "non-member is a 403");
    assert_eq!(
        counter.count_containing("mls_group"),
        1,
        "non-member path is still one SELECT"
    );

    // (c) missing group: 404 wins even though the signer is a member of nothing.
    counter.reset();
    let missing = fresh_group_id();
    let mpath = format!("/mls/groups/{missing}/messages");
    let err = send_message(
        State(state.clone()),
        support::signed_headers(&creator, "post", &mpath),
        Path(missing),
        Bytes::from_static(b"not json"),
    )
    .await
    .expect_err("missing group must 404");
    assert_eq!(
        err.code, 404,
        "missing group is a 404 (precedence over 403)"
    );

    scratch.drop().await;
}
