//! Rust-backed WebTransport server transport for Decentraland Pulse.
//!
//! [`host::Host`] is the Rust API: a thin, generic wrapper over `web-transport-quinn`
//! exposing an ENet-shaped, synchronously-polled model (create -> service -> send -> destroy).
//! Pulse channel semantics (length framing, datagram sequencing) live in the C# layer; this
//! crate is a dumb byte pipe for the reliable stream and for datagrams.
//!
//! [`client::Client`] is the matching client (used by the .NET test client to connect to a
//! Pulse server over WebTransport). [`cabi`] exposes both over a C ABI for P/Invoke from .NET.

pub mod cabi;
pub mod client;
pub mod host;
