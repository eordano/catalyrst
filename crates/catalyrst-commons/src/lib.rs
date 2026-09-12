#[cfg(feature = "cache")]
pub mod cache;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "worker")]
pub mod worker;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
