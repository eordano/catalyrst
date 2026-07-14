use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use rusty_enet as enet;

use crate::transport::packet::Packet;
use crate::transport::peer::PeerId;

const SERVICE_POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub bind: SocketAddr,
    pub max_peers: usize,
    pub channel_limit: u8,
}

#[derive(Debug)]
pub enum Event {
    Connect {
        peer: PeerId,
        /// Source IP, when the transport knows it at connect time (WebTransport). ENet leaves
        /// this `None`; the server resolves it via [`Host::peer_ip`].
        ip: Option<String>,
    },
    Receive {
        peer: PeerId,
        channel: u8,
        packet: Packet,
    },
    Disconnect {
        peer: PeerId,
    },
    /// Transport-level malformed input (a stream frame past the cap, or a datagram too short
    /// to hold its header). Only WebTransport emits this — ENet frames packets itself. The
    /// server charges it against the shared corrupted-packet budget, exactly like a protobuf
    /// decode failure, and disconnects the peer on exhaustion.
    Corrupt {
        peer: PeerId,
    },
}

pub struct Host {
    inner: enet::Host<UdpSocket>,
}

fn to_enet_peer(id: PeerId) -> enet::PeerID {
    enet::PeerID(id as usize)
}

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

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.inner.socket().local_addr()
    }

    pub fn peer_ip(&mut self, peer: PeerId) -> Option<String> {
        self.inner
            .get_peer_mut(to_enet_peer(peer))
            .and_then(|p| p.address())
            .map(|addr| addr.ip().to_string())
    }

    pub fn connect(&mut self, addr: SocketAddr, channels: u8) -> std::io::Result<PeerId> {
        let peer = self
            .inner
            .connect(addr, channels as usize, 0)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
        Ok(peer.id().0 as PeerId)
    }

    pub async fn service(&mut self) -> std::io::Result<Option<Event>> {
        let event = self
            .inner
            .service()
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?
            .map(|e| e.no_ref());
        match event {
            Some(enet::EventNoRef::Connect { peer, .. }) => Ok(Some(Event::Connect {
                peer: peer.0 as PeerId,
                ip: None,
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

    pub async fn disconnect(&mut self, peer: PeerId, reason: u32) -> std::io::Result<()> {
        if let Some(p) = self.inner.get_peer_mut(to_enet_peer(peer)) {
            p.disconnect(reason);
            self.inner.flush();
        }
        Ok(())
    }

    pub async fn disconnect_now(&mut self, peer: PeerId, reason: u32) -> std::io::Result<()> {
        if let Some(p) = self.inner.get_peer_mut(to_enet_peer(peer)) {
            p.disconnect_now(reason);
            self.inner.flush();
        }
        Ok(())
    }

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
