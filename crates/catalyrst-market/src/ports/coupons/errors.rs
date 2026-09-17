use super::types::{MAX_COUPON_COLLECTIONS, MAX_DISCOUNT_PPM, MIN_DISCOUNT_PPM};

/// Upstream spells these as twelve error classes (ports/coupons/errors.ts); the message text is
/// what creators and support see, so it is reproduced verbatim.
#[derive(Debug)]
pub enum CouponError {
    InvalidBody(String),
    InvalidSigner,
    UnsupportedChain(i64),
    InvalidNetwork,
    AlreadyUnusable(String),
    InvalidAddress,
    NotAllowed,
    InvalidDiscount(String),
    InvalidCollections(String),
    InvalidChecks(String),
    InvalidSignature,
    InvalidSignatureIndex,
    NotCollectionCreator(String),
    Duplicate,
    NotFound(String),
    Db(sqlx::Error),
    Internal(String),
}

pub const ONLY_PERCENTAGE_DISCOUNTS: &str = "Only percentage discounts are supported";
pub const NO_COLLECTIONS: &str = "A coupon must cover at least one collection";
pub const AT_LEAST_ONE_USE: &str = "A coupon must allow at least one use";
pub const EXPIRATION_IN_THE_FUTURE: &str = "Coupon expiration date must be in the future";
pub const EFFECTIVE_BEFORE_EXPIRY: &str = "Coupon should be effective before it expires";
pub const SCHEDULED_TOO_FAR_AHEAD: &str = "A sale may not be scheduled more than 30 days ahead";
pub const RUNS_TOO_LONG: &str = "A sale may run for at most 30 days";
pub const NO_ALLOWED_ROOT: &str = "A coupon cannot restrict who may use it";
pub const NO_EXTERNAL_CHECKS: &str = "A coupon cannot carry external checks";
pub const COUPON_NOT_ALLOWED: &str =
    "The coupon manager the signature was made against does not accept this coupon contract";
pub const ALREADY_CANCELLED: &str = "This coupon was already cancelled on chain";
pub const NO_USES_LEFT: &str = "This coupon has no uses left";

pub fn discount_out_of_bounds() -> String {
    format!(
        "The discount must be between {}% and {}%",
        MIN_DISCOUNT_PPM / 10_000,
        MAX_DISCOUNT_PPM / 10_000
    )
}

pub fn too_many_collections() -> String {
    format!("A coupon may cover at most {MAX_COUPON_COLLECTIONS} collections")
}

impl std::fmt::Display for CouponError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CouponError::InvalidBody(why) => write!(f, "{why}"),
            CouponError::InvalidSigner => write!(f, "Coupon and request signer do not match"),
            CouponError::UnsupportedChain(chain_id) => {
                write!(f, "Coupons are not available on chain {chain_id}")
            }
            CouponError::InvalidNetwork => {
                write!(f, "The coupon network does not match its chain")
            }
            CouponError::AlreadyUnusable(why) => write!(f, "{why}"),
            CouponError::InvalidAddress => write!(
                f,
                "The coupon address is not the collection discount coupon of this chain"
            ),
            CouponError::NotAllowed => write!(f, "{COUPON_NOT_ALLOWED}"),
            CouponError::InvalidDiscount(why) => write!(f, "{why}"),
            CouponError::InvalidCollections(why) => write!(f, "{why}"),
            CouponError::InvalidChecks(why) => write!(f, "{why}"),
            CouponError::InvalidSignature => write!(f, "Invalid coupon signature"),
            CouponError::InvalidSignatureIndex => write!(
                f,
                "The coupon signature indexes do not match the current on-chain values"
            ),
            CouponError::NotCollectionCreator(collection) => {
                write!(
                    f,
                    "The signer is not the creator of collection {collection}"
                )
            }
            CouponError::Duplicate => write!(f, "This coupon already exists"),
            CouponError::NotFound(id) => write!(f, "Coupon not found for id {id}"),
            CouponError::Db(e) => write!(f, "database error: {e}"),
            CouponError::Internal(why) => write!(f, "{why}"),
        }
    }
}

impl From<sqlx::Error> for CouponError {
    fn from(e: sqlx::Error) -> Self {
        CouponError::Db(e)
    }
}
