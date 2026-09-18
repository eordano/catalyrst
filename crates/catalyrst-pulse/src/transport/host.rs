use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

use rusty_enet as enet;

use super::application_budget::{ApplicationBudget, ApplicationPayload};
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
    /// to hold its header). Only WebTransport emits this -- ENet frames packets itself. The
    /// server charges it against the shared corrupted-packet budget, exactly like a protobuf
    /// decode failure, and disconnects the peer on exhaustion.
    Corrupt {
        peer: PeerId,
    },
}

pub struct Host {
    inner: enet::Host<ReadySocket>,
    application_global: Arc<tokio::sync::Semaphore>,
    application_peers: HashMap<PeerId, ApplicationBudget>,
}

struct ReadySocket(UdpSocket);

impl enet::Socket for ReadySocket {
    type Address = SocketAddr;
    type Error = std::io::Error;

    fn init(&mut self, _: enet::SocketOptions) -> std::io::Result<()> {
        self.0.set_broadcast(true)
    }

    fn send(&mut self, address: SocketAddr, buffer: &[u8]) -> std::io::Result<usize> {
        match self.0.try_send_to(buffer, address) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            result => result,
        }
    }

    fn receive(
        &mut self,
        buffer: &mut [u8; enet::MTU_MAX],
    ) -> std::io::Result<Option<(SocketAddr, enet::PacketReceived)>> {
        match self.0.try_recv_from(buffer) {
            Ok((len, address)) => Ok(Some((address, enet::PacketReceived::Complete(len)))),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
thread_local! {
    pub static HOST_FLUSH_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
        let socket = UdpSocket::bind(config.bind).await?;
        socket.writable().await?;
        let settings = enet::HostSettings {
            peer_limit: config.max_peers,
            channel_limit: config.channel_limit as usize,
            ..Default::default()
        };
        let inner = enet::Host::new(ReadySocket(socket), settings)
            .map_err(|e| std::io::Error::other(format!("enet host: {e:?}")))?;
        Ok(Self {
            inner,
            application_global: ApplicationBudget::global(),
            application_peers: HashMap::new(),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.inner.socket().0.local_addr()
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
            Some(enet::EventNoRef::Connect { peer, .. }) => {
                self.application_peers.remove(&(peer.0 as PeerId));
                Ok(Some(Event::Connect {
                    peer: peer.0 as PeerId,
                    ip: None,
                }))
            }
            Some(enet::EventNoRef::Disconnect { peer, .. }) => {
                self.application_peers.remove(&(peer.0 as PeerId));
                Ok(Some(Event::Disconnect {
                    peer: peer.0 as PeerId,
                }))
            }
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
                tokio::select! {
                    ready = self.inner.socket().0.readable() => { ready?; }
                    _ = tokio::time::sleep(SERVICE_POLL) => {}
                }
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
        self.application_peers.remove(&peer);
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
        Ok(())
    }

    pub async fn send_application(&mut self, peer: PeerId, packet: Packet) -> std::io::Result<()> {
        let kind = packet_kind(&packet);
        let p = self
            .inner
            .get_peer_mut(to_enet_peer(peer))
            .filter(|peer| peer.state() == enet::PeerState::Connected)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "application peer unavailable",
                )
            })?;
        let budget = self
            .application_peers
            .entry(peer)
            .or_insert_with(|| ApplicationBudget::new(self.application_global.clone()));
        let permit = budget.reserve(packet.data.len())?;
        let raw = enet::Packet::new(
            Box::new(ApplicationPayload {
                bytes: packet.data,
                _permit: permit,
            }),
            kind,
        );
        p.send(packet.channel, &raw)
            .map_err(|error| std::io::Error::other(format!("application send: {error:?}")))
    }

    /// One `enet_host_flush` for the whole tick's outbox; `send` only queues.
    pub fn flush(&mut self) {
        #[cfg(test)]
        HOST_FLUSH_CALLS.with(|c| c.set(c.get() + 1));
        self.inner.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::packet::Packet;

    fn loopback() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    #[tokio::test]
    async fn flush_called_once_per_tick() {
        let mut server = Host::bind(HostConfig {
            bind: loopback(),
            max_peers: 8,
            channel_limit: 8,
        })
        .await
        .unwrap();
        let server_addr = server.local_addr().unwrap();

        let mut client = Host::bind(HostConfig {
            bind: loopback(),
            max_peers: 8,
            channel_limit: 8,
        })
        .await
        .unwrap();
        client.connect(server_addr, 8).unwrap();

        let mut server_peer: Option<PeerId> = None;
        for _ in 0..200 {
            if let Some(Event::Connect { peer, .. }) = server.service().await.unwrap() {
                server_peer = Some(peer);
                break;
            }
            let _ = client.service().await.unwrap();
        }
        let peer = server_peer.expect("enet loopback handshake did not complete");

        HOST_FLUSH_CALLS.with(|c| c.set(0));
        const M: usize = 20;
        for _ in 0..M {
            server
                .send(peer, Packet::reliable(0, vec![1u8, 2, 3]))
                .await
                .unwrap();
        }
        server.flush();

        assert_eq!(
            HOST_FLUSH_CALLS.with(|c| c.get()),
            1,
            "expected a single coalesced flush for the whole tick, not one per message"
        );
    }
}
