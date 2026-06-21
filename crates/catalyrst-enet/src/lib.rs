//! ENet protocol (reliable UDP) — the transport powering `catalyrst-pulse`.
//!
//! The host is a thin async adapter ([`host`]) over the [`rusty_enet`] crate, a
//! direct port of lsalzman/enet 1.3.x that is byte-for-byte wire-compatible with
//! the native ENet that upstream `decentraland/Pulse` P/Invokes. Adopting a
//! wire-faithful ENet (rather than a hand-rolled approximation) is what makes the
//! datagram header (12-bit peer index + session/COMPRESSED/SENT_TIME flag bits
//! packed into `peerID`, variable 2/4-byte length), the CONNECT/VERIFY_CONNECT
//! handshake (random `connectID` echo), reliable windows + ack/retransmit,
//! fragmentation/reassembly, unsequenced groups, MTU `[576,4096]` negotiation and
//! optional CRC32 checksum byte-exact against a real client.
//!
//! [`protocol`] retains the standalone Rust wire codec (header/command/connect
//! bodies) as a documented reference with round-trip tests; it is not on the live
//! send/receive path now that `rusty_enet` owns the wire.

pub mod host;
pub mod packet;
pub mod peer;
pub mod protocol;

pub use host::{Event, Host, HostConfig};
pub use packet::{Packet, PacketFlags};
pub use peer::{Peer, PeerId, PeerState};

/// ENet protocol version this implementation targets.
pub const ENET_PROTOCOL_VERSION: u8 = 1;
