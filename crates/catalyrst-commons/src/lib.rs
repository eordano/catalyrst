//! Building blocks shared by the catalyrst service crates. Every module sits behind
//! its own feature so a consumer pulls in only the dependencies it actually uses.

#[cfg(feature = "cache")]
pub mod cache;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "worker")]
pub mod worker;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
