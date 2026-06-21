//! Pulse — real-time social/MMO server over ENet (port of `decentraland/Pulse`).
//!
//! `catalyrst-enet` provides the (wire-faithful) transport; this crate is the
//! application layer the Unity client speaks: the signed auth-chain handshake
//! ([`handshake`]), the prost protobuf `ClientMessage`/`ServerMessage` codec
//! (the generated [`decentraland`] catalog plus the quantization accessors in
//! [`messages`]), and the session/identity routing that fans gameplay state out
//! to observers ([`server`]).

pub mod handshake;
pub mod hardening;
pub mod interest;
pub mod messages;
pub mod quantize;
pub mod server;
pub mod simulation;
pub mod snapshot;

/// Generated protobuf catalog (prost) from the vendored decentraland/protocol
/// `.proto` files — the full Pulse message wire (byte-identical to upstream).
pub mod decentraland {
    pub mod common {
        include!(concat!(env!("OUT_DIR"), "/decentraland.common.rs"));
    }
    pub mod pulse {
        include!(concat!(env!("OUT_DIR"), "/decentraland.pulse.rs"));
    }
}

pub use handshake::{verify_handshake, HandshakeError, VerifiedHandshake};
pub use server::PulseServer;
