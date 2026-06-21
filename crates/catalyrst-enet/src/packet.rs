//! Application packets handed to/from the host.

use bytes::Bytes;

/// Delivery flags for an outgoing packet (mirrors ENet's `ENetPacketFlag`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PacketFlags(pub u8);

impl PacketFlags {
    /// Reliable, ordered delivery (retransmitted until acked).
    pub const RELIABLE: u8 = 1 << 0;
    /// Unreliable but delivered without a sequence guarantee.
    pub const UNSEQUENCED: u8 = 1 << 1;
    /// Packet may be fragmented even when unreliable.
    pub const UNRELIABLE_FRAGMENT: u8 = 1 << 3;

    pub fn is_reliable(self) -> bool {
        self.0 & Self::RELIABLE != 0
    }
    pub fn is_unsequenced(self) -> bool {
        self.0 & Self::UNSEQUENCED != 0
    }
}

/// A unit of application data on a channel.
#[derive(Debug, Clone)]
pub struct Packet {
    pub channel: u8,
    pub flags: PacketFlags,
    pub data: Bytes,
}

impl Packet {
    pub fn reliable(channel: u8, data: impl Into<Bytes>) -> Self {
        Self {
            channel,
            flags: PacketFlags(PacketFlags::RELIABLE),
            data: data.into(),
        }
    }
    pub fn unreliable(channel: u8, data: impl Into<Bytes>) -> Self {
        Self {
            channel,
            flags: PacketFlags::default(),
            data: data.into(),
        }
    }
    pub fn unsequenced(channel: u8, data: impl Into<Bytes>) -> Self {
        Self {
            channel,
            flags: PacketFlags(PacketFlags::UNSEQUENCED),
            data: data.into(),
        }
    }

    /// Build a received [`Packet`] from a `rusty_enet` packet, recovering the
    /// delivery flags from its `PacketKind` so the application sees the same
    /// reliable/unreliable/unsequenced classification it would have sent.
    pub fn from_enet(channel: u8, raw: &rusty_enet::Packet) -> Self {
        let flags = match raw.kind() {
            rusty_enet::PacketKind::Reliable => PacketFlags(PacketFlags::RELIABLE),
            rusty_enet::PacketKind::Unreliable { sequenced: false }
            | rusty_enet::PacketKind::AlwaysUnreliable { sequenced: false } => {
                PacketFlags(PacketFlags::UNSEQUENCED)
            }
            _ => PacketFlags::default(),
        };
        Self {
            channel,
            flags,
            data: Bytes::copy_from_slice(raw.data()),
        }
    }
}
