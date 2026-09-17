use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;

use super::contracts::CouponMarketplace;
use crate::ports::trades::TradeChecksInput;

/// Percentage bounds a creator may sign, in parts per million: 5% to 70%.
pub const MIN_DISCOUNT_PPM: i64 = 50_000;
pub const MAX_DISCOUNT_PPM: i64 = 700_000;
/// The longest a sale may run. A permanent discount is a price, not a sale, and it would make
/// the list price a fake reference.
pub const MAX_COUPON_DURATION_MS: i64 = 30 * 24 * 60 * 60 * 1000;
pub const MAX_COUPON_COLLECTIONS: usize = 50;
/// The furthest ahead a sale may be scheduled: without a bound, a date far enough out
/// overflows the timestamp the insert builds and is polled forever without ever being buyable.
pub const MAX_COUPON_SCHEDULE_AHEAD_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// How often the on-chain uses/cancellation of live coupons are re-read.
pub const COUPON_STATE_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
/// Page size when a caller does not ask for one.
pub const DEFAULT_PAGE_LIMIT: i64 = 100;

/// The creation body is gated by `validate_creation_schema`, whose `discountType`/`discount`
/// rule is JSON Schema's `integer`: a number with a zero fractional part, however it was
/// spelled. Plain `i64` deserialisation refuses `300000.0`, so the two gates would disagree.
fn integer_number<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let number = serde_json::Number::deserialize(deserializer)?;
    if let Some(integer) = number.as_i64() {
        return Ok(integer);
    }
    number
        .as_f64()
        .filter(|value| value.fract() == 0.0 && value.abs() <= i64::MAX as f64)
        .map(|value| value as i64)
        .ok_or_else(|| serde::de::Error::custom(format!("invalid integer {number}")))
}

/// What the shop posts after the creator signs: the coupon fields the CouponManager hashes,
/// plus the collection list the Merkle root was built from so the root and the proofs can be
/// rebuilt here. `checks` timestamps are in MILLISECONDS, like a trade's, and were signed in
/// seconds.
#[derive(Debug, Deserialize)]
pub struct CouponCreation {
    pub signer: String,
    #[serde(rename = "chainId")]
    pub chain_id: i64,
    pub network: String,
    pub checks: TradeChecksInput,
    #[serde(rename = "couponAddress")]
    pub coupon_address: String,
    #[serde(rename = "discountType", deserialize_with = "integer_number")]
    pub discount_type: i64,
    #[serde(deserialize_with = "integer_number")]
    pub discount: i64,
    pub collections: Vec<String>,
    pub signature: String,
}

/// `revoked` covers the signature indexes moving past the ones the coupon was signed with. A
/// creator who wants every sale to stop calls `increaseSignerSignatureIndex()` -- one
/// argumentless call, against rebuilding each coupon's calldata for `cancelSignature` -- and
/// the contract then refuses all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CouponStatus {
    Scheduled,
    Active,
    Ended,
    Cancelled,
    Exhausted,
    Revoked,
}

/// Consumed uses, cancellation and index revocation as last read from the CouponManager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CouponState {
    pub uses: i64,
    pub cancelled: bool,
    pub revoked: bool,
    #[serde(rename = "checkedAt")]
    pub checked_at: i64,
}

/// What the CouponManager reports for one signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponChainState {
    pub uses: i64,
    pub cancelled: bool,
}

/// The signature indexes the manager is currently at, for the contract and for one signer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponChainIndexes {
    pub contract_signature_index: u64,
    pub signer_signature_index: u64,
}

/// What gets persisted: the manager's own state plus whether the signature indexes have moved
/// past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponStoredState {
    pub uses: i64,
    pub cancelled: bool,
    pub revoked: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Coupon {
    pub id: String,
    pub signer: String,
    #[serde(rename = "chainId")]
    pub chain_id: i64,
    pub network: String,
    #[schema(value_type = Value)]
    pub checks: JsonValue,
    #[serde(rename = "couponManager")]
    pub coupon_manager: String,
    /// The off-chain marketplace version whose manager the coupon was signed against, the only
    /// one that redeems it. None if that manager left the registry.
    pub marketplace: Option<CouponMarketplace>,
    #[serde(rename = "couponAddress")]
    pub coupon_address: String,
    #[serde(rename = "discountType")]
    pub discount_type: i64,
    pub discount: i64,
    pub root: String,
    pub collections: Vec<String>,
    pub signature: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    pub state: Option<CouponState>,
    pub status: CouponStatus,
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbCouponWithState {
    pub id: String,
    pub network: String,
    pub chain_id: i32,
    pub signer: String,
    pub signature: String,
    pub state_key: String,
    pub coupon_manager: String,
    pub coupon_address: String,
    pub checks: JsonValue,
    pub discount_type: i16,
    pub discount_ppm: i32,
    pub root: String,
    pub collections: Vec<String>,
    pub effective_since: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub state_uses: Option<i32>,
    pub state_cancelled: Option<bool>,
    pub state_revoked: Option<bool>,
    pub state_checked_at: Option<DateTime<Utc>>,
}

/// The fields of the stored `checks` blob the refresh pass and the status derivation read back.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct StoredChecks {
    pub uses: i64,
    #[serde(rename = "contractSignatureIndex")]
    pub contract_signature_index: u64,
    #[serde(rename = "signerSignatureIndex")]
    pub signer_signature_index: u64,
}

impl DbCouponWithState {
    pub fn stored_checks(&self) -> Option<StoredChecks> {
        serde_json::from_value(self.checks.clone()).ok()
    }
}

/// How much of a creator's coupon list to return. Bounded so a prolific creator cannot ask for
/// all of it.
#[derive(Debug, Clone, Copy, Default)]
pub struct CouponPagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
