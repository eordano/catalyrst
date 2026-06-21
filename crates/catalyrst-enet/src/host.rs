//! The ENet host — a thin async adapter over [`rusty_enet`].
//!
//! `rusty_enet` is a direct port of lsalzman/enet 1.3.x and is byte-for-byte
//! wire-compatible with the native ENet that `decentraland/Pulse` P/Invokes, so
//! using it (rather than a hand-rolled approximation) is what makes the datagram
//! header, connect handshake, reliable windows, acks, fragmentation and
//! unsequenced groups byte-exact against a real client. This module keeps
//! `catalyrst-pulse`'s existing public surface ([`Host`], [`Event`], [`Packet`],
//! [`HostConfig`]) and translates it onto the synchronous `rusty_enet::Host`
//! driven on the async task (its `std::net::UdpSocket` socket is non-blocking, so
//! `service()` never parks the executor; we yield between idle polls).

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use rusty_enet as enet;

use crate::packet::Packet;
use crate::peer::PeerId;

/// How long an idle [`Host::service`] call yields before returning `None`, giving
/// the caller's loop a chance to run and ENet's timers (retransmit/ping) to fire.
const SERVICE_POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub bind: SocketAddr,
    pub max_peers: usize,
    pub channel_limit: u8,
}

/// A high-level event surfaced by [`Host::service`].
#[derive(Debug)]
pub enum Event {
    Connect {
        peer: PeerId,
    },
    Receive {
        peer: PeerId,
        channel: u8,
        packet: Packet,
    },
    Disconnect {
        peer: PeerId,
    },
}

/// An ENet host. Owns the UDP socket and the `rusty_enet` peer table.
pub struct Host {
    inner: enet::Host<UdpSocket>,
}

/// Translate our [`PeerId`] (`u16`) into a `rusty_enet::PeerID` (`usize`). ENet
/// peer indices are 12-bit (`PROTOCOL_MAXIMUM_PEER_ID`), so the round-trip is lossless.
fn to_enet_peer(id: PeerId) -> enet::PeerID {
    enet::PeerID(id as usize)
}

/// Map a delivered [`Packet`]'s flags to the matching `rusty_enet::PacketKind`.
/// Reliable → ordered+retransmitted; unsequenced → unreliable-unsequenced;
/// otherwise unreliable-sequenced (Pulse's ch1 high-frequency state).
fn packet_kind(packet: &Packet) -> enet::PacketKind {
    if packet.flags.is_reliable() {
        enet::PacketKind::Reliable
    } else if packet.flags.is_unsequenced() {
        enet::PacketKind::Unreliable { sequenced: false }
    } else {
        enet::PacketKind::Unreliable { sequenced: true }
    }
}

impl Host {
    /// Bind a host on `config.bind` and ready it to accept connections.
    pub async fn bind(config: HostConfig) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(config.bind)?;
        let settings = enet::HostSettings {
            peer_limit: config.max_peers,
            channel_limit: config.channel_limit as usize,
            ..Default::default()
        };
        let inner = enet::Host::new(socket, settings)
            .map_err(|e| std::io::Error::other(format!("enet host: {e:?}")))?;
        Ok(Self { inner })
    }

    /// The local address the host is bound to.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.inner.socket().local_addr()
    }

    /// The remote source IP of `peer` (without port), used by the pre-auth per-IP
    /// admission quota (upstream `Peer.IP`). `None` for an unknown / never-connected
    /// peer. The port is dropped so all connections from one host share a quota.
    pub fn peer_ip(&mut self, peer: PeerId) -> Option<String> {
        self.inner
            .get_peer_mut(to_enet_peer(peer))
            .and_then(|p| p.address())
            .map(|addr| addr.ip().to_string())
    }

    /// Initiate an outgoing connection to `addr` over `channels` channels. Returns
    /// the local [`PeerId`] for the pending peer (a real [`Event::Connect`] is
    /// surfaced from [`Host::service`] once the handshake completes).
    pub fn connect(&mut self, addr: SocketAddr, channels: u8) -> std::io::Result<PeerId> {
        let peer = self
            .inner
            .connect(addr, channels as usize, 0)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
        Ok(peer.id().0 as PeerId)
    }

    /// Drive the host: dispatch one ENet event if one is queued, otherwise yield
    /// for the poll interval (so the caller's loop ticks and ENet's reliable
    /// timers run) and return `None`. `rusty_enet` performs ack/retransmit,
    /// dedup, fragmentation reassembly and congestion control internally.
    pub async fn service(&mut self) -> std::io::Result<Option<Event>> {
        // `rusty_enet::service` is non-blocking (its std `UdpSocket` is set
        // non-blocking), so this does not park the tokio executor.
        let event = self
            .inner
            .service()
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?
            .map(|e| e.no_ref());
        match event {
            Some(enet::EventNoRef::Connect { peer, .. }) => Ok(Some(Event::Connect {
                peer: peer.0 as PeerId,
            })),
            Some(enet::EventNoRef::Disconnect { peer, .. }) => Ok(Some(Event::Disconnect {
                peer: peer.0 as PeerId,
            })),
            Some(enet::EventNoRef::Receive {
                peer,
                channel_id,
                packet,
            }) => {
                let packet = Packet::from_enet(channel_id, &packet);
                Ok(Some(Event::Receive {
                    peer: peer.0 as PeerId,
                    channel: channel_id,
                    packet,
                }))
            }
            None => {
                tokio::time::sleep(SERVICE_POLL).await;
                Ok(None)
            }
        }
    }

    /// Request an ENet disconnect of `peer`, carrying `reason` in the 32-bit
    /// ENet disconnect-data field so the client surfaces a reason code rather
    /// than a silent slot reclaim (`Peer.Disconnect((uint)DisconnectReason)` in
    /// upstream Pulse). The teardown is graceful — queued outgoing packets are
    /// still acked and a real [`Event::Disconnect`] is surfaced from
    /// [`Host::service`] once it completes, so the existing cleanup path runs.
    /// A stale/unknown peer id is a no-op (the slot may already be gone).
    pub async fn disconnect(&mut self, peer: PeerId, reason: u32) -> std::io::Result<()> {
        if let Some(p) = self.inner.get_peer_mut(to_enet_peer(peer)) {
            p.disconnect(reason);
            self.inner.flush();
        }
        Ok(())
    }

    /// Force an immediate ENet disconnect of `peer` carrying `reason`
    /// (`Peer.DisconnectNow((uint)DisconnectReason)`). Unlike [`Host::disconnect`]
    /// no [`Event::Disconnect`] is surfaced and the slot is reset inline — used on
    /// the no-allowance paths (gross oversize / pre-auth refusal) where a queued
    /// teardown propagates too slowly to outpace an attack. Unknown peer is a no-op.
    pub async fn disconnect_now(&mut self, peer: PeerId, reason: u32) -> std::io::Result<()> {
        if let Some(p) = self.inner.get_peer_mut(to_enet_peer(peer)) {
            p.disconnect_now(reason);
            self.inner.flush();
        }
        Ok(())
    }

    /// Queue and transmit `packet` to `peer`. A stale/unknown peer id is a no-op
    /// (the session may have already torn down). Reliable retransmit/ack is owned
    /// by `rusty_enet`; we flush so the datagram leaves on the same tick.
    pub async fn send(&mut self, peer: PeerId, packet: Packet) -> std::io::Result<()> {
        let kind = packet_kind(&packet);
        let raw = enet::Packet::new(packet.data.as_ref(), kind);
        let Some(p) = self.inner.get_peer_mut(to_enet_peer(peer)) else {
            return Ok(());
        };
        if p.state() != enet::PeerState::Connected {
            return Ok(());
        }
        let _ = p.send(packet.channel, &raw);
        self.inner.flush();
        Ok(())
    }
}
