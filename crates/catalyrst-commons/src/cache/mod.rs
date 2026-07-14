//! TTL caches. [`TtlCell`] is the single-slot form (one value per process, refreshed on
//! read); [`TtlMap`] is the keyed form with in-flight coalescing and bulk invalidation.

mod cell;
mod map;

pub use cell::TtlCell;
pub use map::TtlMap;
