#![allow(clippy::all)]
#![allow(unused_qualifications)]

pub use prost::DecodeError as ProtocolDecodeError;
pub use prost::Message as ProtocolMessage;

pub mod decentraland {
    pub mod quests {
        include!(concat!(env!("OUT_DIR"), "/decentraland.quests.rs"));
    }
}

pub use decentraland::quests::*;
