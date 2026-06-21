//! Generated protobuf + dcl-rpc bindings for `decentraland.quests`
//! (vendored `definitions.proto`, pinned to protocol commit
//! 03626d76db879afcdfd4fbcdc0342a04e5b4f663 — byte-identical to upstream).
//!
//! The generated types carry `#[serde(rename_all = "camelCase")]` so the REST
//! surface serializes byte-compatibly with the upstream protobuf JSON shape.
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
