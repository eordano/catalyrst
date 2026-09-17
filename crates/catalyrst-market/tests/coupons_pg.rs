use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use alloy_primitives::B256;
use async_trait::async_trait;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_market::ports::coupons::merkle::collections_root;
use catalyrst_market::ports::coupons::signature::{
    coupon_signing_hash, encode_coupon_data, DISCOUNT_TYPE_RATE,
};
use catalyrst_market::ports::coupons::types::{
    CouponChainIndexes, CouponChainState, CouponPagination,
};
use catalyrst_market::ports::coupons::{
    ChainReadError, CouponChainReader, CouponError, CouponStatus, CouponsComponent,
};
use catalyrst_market::ports::trades::MATIC_AMOY;
use serde_json::{json, Value};
use sqlx::PgPool;

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const COLLECTION: &str = "0x4c09495cd2d4e3d3fa2808eb655d013de426157b";
const COUPON_ADDRESS: &str = "0x4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9";
const COUPON_MANAGER: &str = "0x6c956587d9fe70032781edcdc626310648575382";
const PREVIOUS_COUPON_MANAGER: &str = "0xa40b1d129b8906888720686f3a01921ddf37716f";
const SIGNED_USES: i64 = 10;

/// Answers whatever the test wants the CouponManager to be reporting, so the persistence and
/// refresh paths can be exercised without an RPC endpoint.
struct StubChain {
    allowed: bool,
    indexes: CouponChainIndexes,
    state: CouponChainState,
    index_reads: Arc<AtomicUsize>,
}

impl StubChain {
    fn fresh() -> Self {
        Self {
            allowed: true,
            indexes: CouponChainIndexes {
                contract_signature_index: 0,
                signer_signature_index: 0,
            },
            state: CouponChainState {
                uses: 0,
                cancelled: false,
            },
            index_reads: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl CouponChainReader for StubChain {
    async fn read_coupon_allowed(
        &self,
        _chain_id: i64,
        _coupon_manager: &str,
        _coupon: &str,
    ) -> Result<bool, ChainReadError> {
        Ok(self.allowed)
    }

    async fn read_indexes(
        &self,
        _chain_id: i64,
        _coupon_manager: &str,
        _signer: &str,
    ) -> Result<CouponChainIndexes, ChainReadError> {
        self.index_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.indexes)
    }

    async fn read_state(
        &self,
        _chain_id: i64,
        _coupon_manager: &str,
        _state_keys: &[String],
    ) -> Result<CouponChainState, ChainReadError> {
        Ok(self.state)
    }
}

async fn build_scratch() -> Option<ScratchDb> {
    let scratch = ScratchDb::builder(PG_VAR, "cg_mkt_coupons")
        .schemas(["marketplace", "squid_marketplace"])
        .build()
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0013_coupons.sql"))
        .execute(&scratch.pool)
        .await
        .expect("0013 applies");
    sqlx::raw_sql(
        "CREATE TABLE squid_marketplace.collection (id text PRIMARY KEY, creator text, chain_id integer)",
    )
    .execute(&scratch.pool)
    .await
    .expect("the squid collection table");
    Some(scratch)
}

async fn seed_collection(pool: &PgPool, collection: &str, creator: &str) {
    sqlx::query(
        "INSERT INTO squid_marketplace.collection (id, creator, chain_id) VALUES ($1, $2, $3)",
    )
    .bind(collection)
    .bind(creator.to_lowercase())
    .bind(MATIC_AMOY as i32)
    .execute(pool)
    .await
    .unwrap();
}

fn component(pool: &PgPool, chain: StubChain) -> CouponsComponent {
    CouponsComponent::new(pool.clone(), pool.clone(), Arc::new(chain))
}

fn wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[0] = 1;
    key[31] = seed;
    PrivateKeySigner::from_bytes(&B256::from(key)).expect("test key")
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn body(signer: &str, now: i64, salt: &str) -> Value {
    json!({
        "signer": signer,
        "chainId": MATIC_AMOY,
        "network": "MATIC",
        "checks": {
            "uses": SIGNED_USES,
            "expiration": now + 7 * DAY_MS,
            "effective": now - 1000,
            "salt": format!("0x{}", salt.repeat(32)),
            "contractSignatureIndex": 0,
            "signerSignatureIndex": 0,
            "allowedRoot": format!("0x{}", "00".repeat(32)),
            "externalChecks": []
        },
        "couponAddress": COUPON_ADDRESS,
        "discountType": DISCOUNT_TYPE_RATE,
        "discount": 300_000,
        "collections": [COLLECTION],
        "signature": format!("0x{}", "00".repeat(65))
    })
}

/// Signs the body the way the shop does, so every refusal below is about the check it names.
fn signed(
    creator: &PrivateKeySigner,
    value: Value,
) -> catalyrst_market::ports::coupons::CouponCreation {
    signed_against(creator, value, 0)
}

/// `manager` indexes the chain's deployments newest first, so 1 signs against the manager of the
/// previous off-chain marketplace, which is still live.
fn signed_against(
    creator: &PrivateKeySigner,
    mut value: Value,
    manager: usize,
) -> catalyrst_market::ports::coupons::CouponCreation {
    let address = creator.address().to_string().to_lowercase();
    value["signer"] = json!(address);
    let mut coupon: catalyrst_market::ports::coupons::CouponCreation =
        serde_json::from_value(value).expect("body parses");
    let contracts = catalyrst_market::ports::coupons::coupon_contracts(coupon.chain_id)[manager];
    let root = collections_root(&coupon.collections).expect("a root");
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
    coupon
}

#[tokio::test]
async fn a_signed_coupon_is_stored_once_and_served_back_by_signer_and_by_id() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(41);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let coupons = component(&scratch.pool, StubChain::fresh());
    let now = now_ms();

    let created = coupons
        .add_coupon(signed(&creator, body("", now, "11")), &signer, now)
        .await
        .expect("a well formed coupon is stored");
    assert_eq!(created.signer, signer);
    assert_eq!(created.network, "MATIC");
    assert_eq!(created.chain_id, MATIC_AMOY);
    assert_eq!(created.coupon_manager, COUPON_MANAGER);
    assert_eq!(created.coupon_address, COUPON_ADDRESS);
    assert_eq!(created.discount, 300_000);
    assert_eq!(created.collections, vec![COLLECTION.to_string()]);
    assert_eq!(created.status, CouponStatus::Active);
    assert_eq!(created.state.expect("a state was read").uses, 0);

    let fetched = coupons.get_coupon(&created.id).await.expect("served by id");
    assert_eq!(fetched.signature, created.signature);
    assert_eq!(fetched.root, created.root);
    assert_eq!(fetched.status, CouponStatus::Active);

    let listed = coupons
        .get_coupons_by_signer(&signer.to_uppercase(), CouponPagination::default())
        .await
        .expect("served by signer");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);

    let other = coupons
        .get_coupons_by_signer(
            "0x0000000000000000000000000000000000000001",
            CouponPagination::default(),
        )
        .await
        .expect("another creator's list");
    assert!(other.is_empty());

    scratch.drop().await;
}

#[tokio::test]
async fn the_same_signature_cannot_be_stored_twice() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(43);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let coupons = component(&scratch.pool, StubChain::fresh());
    let now = now_ms();

    coupons
        .add_coupon(signed(&creator, body("", now, "22")), &signer, now)
        .await
        .expect("the first post is stored");
    let err = coupons
        .add_coupon(signed(&creator, body("", now, "22")), &signer, now)
        .await
        .expect_err("the second post is refused");
    assert!(matches!(err, CouponError::Duplicate));
    assert_eq!(err.to_string(), "This coupon already exists");

    scratch.drop().await;
}

#[tokio::test]
async fn a_creator_can_only_discount_their_own_collections() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(45);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &wallet(46).address().to_string()).await;
    let coupons = component(&scratch.pool, StubChain::fresh());
    let now = now_ms();

    let err = coupons
        .add_coupon(signed(&creator, body("", now, "33")), &signer, now)
        .await
        .expect_err("someone else's collection is refused");
    assert!(matches!(err, CouponError::NotCollectionCreator(ref c) if c == COLLECTION));

    scratch.drop().await;
}

#[tokio::test]
async fn a_signature_index_the_manager_has_moved_past_is_refused() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(47);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let mut chain = StubChain::fresh();
    chain.indexes.signer_signature_index = 1;
    let coupons = component(&scratch.pool, chain);
    let now = now_ms();

    let err = coupons
        .add_coupon(signed(&creator, body("", now, "44")), &signer, now)
        .await
        .expect_err("a stale index is refused");
    assert!(matches!(err, CouponError::InvalidSignatureIndex));

    scratch.drop().await;
}

#[tokio::test]
async fn a_coupon_the_manager_reports_as_used_up_or_cancelled_is_refused() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(49);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let now = now_ms();

    let mut spent = StubChain::fresh();
    spent.state.uses = SIGNED_USES;
    let err = component(&scratch.pool, spent)
        .add_coupon(signed(&creator, body("", now, "55")), &signer, now)
        .await
        .expect_err("a spent coupon is refused");
    assert!(matches!(err, CouponError::AlreadyUnusable(_)));

    let mut cancelled = StubChain::fresh();
    cancelled.state.cancelled = true;
    let err = component(&scratch.pool, cancelled)
        .add_coupon(signed(&creator, body("", now, "66")), &signer, now)
        .await
        .expect_err("a cancelled coupon is refused");
    assert_eq!(
        err.to_string(),
        "This coupon was already cancelled on chain"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn the_refresh_pass_records_uses_cancellation_and_index_revocation() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(51);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let now = now_ms();
    let created = component(&scratch.pool, StubChain::fresh())
        .add_coupon(signed(&creator, body("", now, "77")), &signer, now)
        .await
        .expect("stored");

    let mut used = StubChain::fresh();
    used.state.uses = 3;
    let refreshed = component(&scratch.pool, used)
        .refresh_state()
        .await
        .expect("a refresh tick");
    assert_eq!(refreshed, 1);
    let after_use = component(&scratch.pool, StubChain::fresh())
        .get_coupon(&created.id)
        .await
        .expect("served after the refresh");
    assert_eq!(after_use.state.expect("a state").uses, 3);
    assert_eq!(after_use.status, CouponStatus::Active);

    let mut spent = StubChain::fresh();
    spent.state.uses = SIGNED_USES;
    component(&scratch.pool, spent)
        .refresh_state()
        .await
        .expect("a refresh tick");
    let after_spend = component(&scratch.pool, StubChain::fresh())
        .get_coupon(&created.id)
        .await
        .expect("served after the refresh");
    assert_eq!(after_spend.status, CouponStatus::Exhausted);

    // increaseSignerSignatureIndex() leaves `cancelled` false while the contract refuses the
    // coupon, so the revoked flag is the only thing that separates the two.
    let mut revoked = StubChain::fresh();
    revoked.indexes.signer_signature_index = 1;
    component(&scratch.pool, revoked)
        .refresh_state()
        .await
        .expect("a refresh tick");
    let after_revoke = component(&scratch.pool, StubChain::fresh())
        .get_coupon(&created.id)
        .await
        .expect("served after the refresh");
    assert!(after_revoke.state.expect("a state").revoked);
    assert_eq!(after_revoke.status, CouponStatus::Revoked);

    // A revoked coupon is left out of the next batch: the chain cannot un-revoke it, and a
    // permanent row would slowly starve the coupons whose state still moves.
    let refreshed = component(&scratch.pool, StubChain::fresh())
        .refresh_state()
        .await
        .expect("a refresh tick");
    assert_eq!(refreshed, 0);

    scratch.drop().await;
}

/// Each manager keeps its own allow-list of coupon contracts, and an older one may never have
/// learnt the current CollectionDiscountCoupon, so a coupon it would revert on is refused here
/// rather than shown to buyers.
#[tokio::test]
async fn a_manager_that_does_not_accept_the_coupon_contract_refuses_it() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(53);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let now = now_ms();

    let mut disallowing = StubChain::fresh();
    disallowing.allowed = false;
    let err = component(&scratch.pool, disallowing)
        .add_coupon(signed(&creator, body("", now, "88")), &signer, now)
        .await
        .expect_err("a coupon the manager does not accept is refused");
    assert!(matches!(err, CouponError::NotAllowed));
    assert_eq!(
        err.to_string(),
        "The coupon manager the signature was made against does not accept this coupon contract"
    );

    scratch.drop().await;
}

/// The two managers of a chain are both live while clients move over: a coupon signed against
/// the previous marketplace's manager is stored against that manager and reports it.
#[tokio::test]
async fn a_coupon_signed_against_the_previous_manager_is_stored_against_that_manager() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(55);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let coupons = component(&scratch.pool, StubChain::fresh());
    let now = now_ms();

    let created = coupons
        .add_coupon(
            signed_against(&creator, body("", now, "99"), 1),
            &signer,
            now,
        )
        .await
        .expect("a coupon signed against the previous manager is stored");
    assert_eq!(created.coupon_manager, PREVIOUS_COUPON_MANAGER);
    assert_eq!(
        created.marketplace,
        Some(catalyrst_market::ports::coupons::CouponMarketplace::OffChainMarketplaceV2)
    );

    let fetched = coupons.get_coupon(&created.id).await.expect("served by id");
    assert_eq!(fetched.coupon_manager, PREVIOUS_COUPON_MANAGER);
    assert_eq!(
        fetched.marketplace,
        Some(catalyrst_market::ports::coupons::CouponMarketplace::OffChainMarketplaceV2)
    );

    let refreshed = component(&scratch.pool, StubChain::fresh())
        .refresh_state()
        .await
        .expect("a refresh tick");
    assert_eq!(refreshed, 1, "the previous manager is still readable");

    scratch.drop().await;
}

/// A manager the registry has stopped listing leaves no EIP-712 domain to rebuild the digest
/// slot from, so the row keeps its last known state instead of being read from an invented slot.
#[tokio::test]
async fn a_row_naming_a_manager_no_longer_deployed_is_left_alone_by_the_refresh() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let creator = wallet(57);
    let signer = creator.address().to_string().to_lowercase();
    seed_collection(&scratch.pool, COLLECTION, &signer).await;
    let now = now_ms();
    let created = component(&scratch.pool, StubChain::fresh())
        .add_coupon(signed(&creator, body("", now, "aa")), &signer, now)
        .await
        .expect("stored");

    sqlx::query("UPDATE marketplace.coupons SET coupon_manager = $1 WHERE id = $2::uuid")
        .bind(format!("0x{}", "99".repeat(20)))
        .bind(&created.id)
        .execute(&scratch.pool)
        .await
        .unwrap();

    let mut used = StubChain::fresh();
    used.state.uses = 4;
    let index_reads = used.index_reads.clone();
    let refreshed = component(&scratch.pool, used)
        .refresh_state()
        .await
        .expect("a refresh tick");
    assert_eq!(refreshed, 0);
    assert_eq!(
        index_reads.load(Ordering::SeqCst),
        0,
        "a row with no slot to read is skipped before the signature indexes are asked for"
    );

    let after = component(&scratch.pool, StubChain::fresh())
        .get_coupon(&created.id)
        .await
        .expect("served after the refresh");
    assert_eq!(after.state.expect("a state").uses, 0);
    assert_eq!(after.marketplace, None);

    scratch.drop().await;
}
