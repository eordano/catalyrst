pub mod batch;
#[cfg(any(test, feature = "fuzzing"))]
pub mod fuzz;
pub mod handshake;
pub mod hardening;
pub mod interest;
pub mod messages;
pub mod metrics;
pub mod quantize;
pub mod server;
pub mod simulation;
pub mod snapshot;
pub mod transport;

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
