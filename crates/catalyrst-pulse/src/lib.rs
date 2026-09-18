pub mod application_relay;
pub mod batch;
pub mod cluster;
#[cfg(any(test, feature = "fuzzing"))]
pub mod fuzz;
pub mod handshake;
pub mod hardening;
pub mod interest;
pub mod messages;
pub mod metrics;
pub mod quantize;
pub mod realm_grids;
pub mod server;
pub mod simulation;
pub mod snapshot;
pub mod transport;
pub mod v4;

pub mod decentraland {
    pub mod common {
        include!(concat!(env!("OUT_DIR"), "/decentraland.common.rs"));
    }
    pub mod pulse {
        include!(concat!(env!("OUT_DIR"), "/decentraland.pulse.rs"));
    }
    pub mod kernel {
        pub mod comms {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/decentraland.kernel.comms.v3.rs"));
            }
        }
    }
}

pub use cluster::{ClusterOptions, ClusterPass, ClusterSession, ClusterTracker};
pub use decentraland::pulse::PeerClusterChange;
pub use handshake::{verify_handshake, HandshakeError, VerifiedHandshake};
pub use realm_grids::RealmSpatialGrids;
pub use server::PulseServer;
