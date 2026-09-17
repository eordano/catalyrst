pub mod chain;
pub mod component;
pub mod contracts;
pub mod errors;
pub mod merkle;
pub mod schema;
pub mod signature;
mod sql;
pub mod types;

#[cfg(test)]
mod component_tests;

pub use chain::{ChainReadError, CouponChainReader, RpcCouponChainReader};
pub use component::{to_coupon, validate_creation, CouponsComponent};
pub use contracts::{coupon_contracts, find_coupon_contracts, CouponContracts, CouponMarketplace};
pub use errors::CouponError;
pub use schema::validate_creation_schema;
pub use types::{
    Coupon, CouponCreation, CouponPagination, CouponState, CouponStatus, DbCouponWithState,
    COUPON_STATE_REFRESH_INTERVAL,
};
