use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver as StdReceiver, Sender as StdSender};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use web_transport::host::{Event as QuicEvent, Host as QuicHost, HostConfig as QuicConfig};

use crate::hardening::DisconnectReason;
use crate::transport::peer::PeerId;
use crate::transport::webtransport::config::WtConfig;
use crate::transport::webtransport::framing::{
    parse_datagram, stream_frame, DatagramDeduper, StreamFrameReader,
};
use crate::transport::webtransport::CHANNEL_SEQUENCED;
use crate::transport::{Event, Packet};

#[derive(Debug, Default)]
pub struct WtMetrics {
    pub datagrams_dropped_stale: AtomicU64,
    pub datagrams_dropped_oversize: AtomicU64,
    pub messages_dropped_oversize: AtomicU64,
    pub peers_refused_full: AtomicU64,
}

enum Outbound {
    Send { peer: u32, packet: Packet },
    Disconnect { peer: u32, reason: u32 },
}

pub struct WtHost {
    outbound_tx: StdSender<Outbound>,
    local_addr: SocketAddr,
    metrics: Arc<WtMetrics>,
}

impl WtHost {
    pub fn start(config: WtConfig) -> anyhow::Result<(WtHost, UnboundedReceiver<Event>)> {
        let ceiling = config.slot_base as u64 + config.slot_capacity as u64;
        if ceiling > u16::MAX as u64 + 1 {
            anyhow::bail!(
                "webtransport slot range [{}, {}) exceeds the PeerId (u16) space",
                config.slot_base,
                ceiling
            );
        }

        let quic = QuicHost::new(QuicConfig {
            bind_addr: config.bind_addr,
            cert_pem: config.cert_pem.clone(),
            key_pem: config.key_pem.clone(),
        })
        .map_err(|e| anyhow::anyhow!("webtransport host bind: {e}"))?;
        let local_addr = quic.local_addr();

        let (events_tx, events_rx) = unbounded_channel::<Event>();
        let (outbound_tx, outbound_rx) = std::sync::mpsc::channel::<Outbound>();
        let metrics = Arc::new(WtMetrics::default());
        let thread_metrics = metrics.clone();

        std::thread::Builder::new()
            .name("pulse-webtransport".into())
            .spawn(move || run_loop(quic, config, events_tx, outbound_rx, thread_metrics))?;

        Ok((
            WtHost {
                outbound_tx,
                local_addr,
                metrics,
            },
            events_rx,
        ))
    }

    pub fn send(&self, peer: u32, packet: Packet) {
        let _ = self.outbound_tx.send(Outbound::Send { peer, packet });
    }

    pub fn disconnect(&self, peer: u32, reason: u32) {
        let _ = self.outbound_tx.send(Outbound::Disconnect { peer, reason });
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn metrics(&self) -> Arc<WtMetrics> {
        self.metrics.clone()
    }
}

struct WtSession {
    our_peer: u32,
    reader: StreamFrameReader,
    deduper: DatagramDeduper,
}

fn run_loop(
    mut quic: QuicHost,
    config: WtConfig,
    events_tx: UnboundedSender<Event>,
    outbound_rx: StdReceiver<Outbound>,
    metrics: Arc<WtMetrics>,
) {
    let mut by_quic: HashMap<u64, WtSession> = HashMap::new();
    let mut quic_by_peer: HashMap<u32, u64> = HashMap::new();
    let mut free_slots: Vec<u32> = (config.slot_base
        ..config.slot_base + config.slot_capacity as u32)
        .rev()
        .collect();

    let timeout = Duration::from_millis(config.service_timeout_ms);

    loop {
        while let Ok(cmd) = outbound_rx.try_recv() {
            match cmd {
                Outbound::Disconnect { peer, reason } => {
                    if let Some(&qid) = quic_by_peer.get(&peer) {
                        quic.disconnect(qid, reason);
                    }
                }
                Outbound::Send { peer, packet } => {
                    let Some(&qid) = quic_by_peer.get(&peer) else {
                        continue;
                    };
                    if packet.channel == CHANNEL_RELIABLE {
                        if packet.data.len() > config.max_message_bytes {
                            metrics
                                .messages_dropped_oversize
                                .fetch_add(1, Ordering::Relaxed);
                            tracing::error!(
                                peer,
                                size = packet.data.len(),
                                cap = config.max_message_bytes,
                                "webtransport: reliable message exceeds max_message_bytes — dropped"
                            );
                        } else {
                            quic.send_stream(qid, &stream_frame(&packet.data));
                        }
                    } else if packet.data.len() > config.max_datagram_bytes {
                        metrics
                            .datagrams_dropped_oversize
                            .fetch_add(1, Ordering::Relaxed);
                        tracing::error!(
                            peer,
                            size = packet.data.len(),
                            cap = config.max_datagram_bytes,
                            "webtransport: unreliable message exceeds datagram cap — dropped"
                        );
                    } else {
                        quic.send_datagram(qid, &packet.data);
                    }
                }
            }
        }

        let Some(event) = quic.service(timeout) else {
            if events_tx.is_closed() {
                shutdown_graceful(&quic, &by_quic);
                return;
            }
            continue;
        };

        let forwarded = match event {
            QuicEvent::Connect {
                peer_id,
                remote_addr,
            } => handle_connect(
                &mut quic,
                &config,
                &metrics,
                &mut by_quic,
                &mut quic_by_peer,
                &mut free_slots,
                &events_tx,
                peer_id,
                remote_addr,
            ),
            QuicEvent::StreamData { peer_id, data } => {
                handle_stream(&mut by_quic, &events_tx, peer_id, &data)
            }
            QuicEvent::Datagram { peer_id, data } => {
                handle_datagram(&metrics, &mut by_quic, &events_tx, peer_id, &data)
            }
            QuicEvent::Disconnect { peer_id, .. } => handle_disconnect(
                &mut by_quic,
                &mut quic_by_peer,
                &mut free_slots,
                &events_tx,
                peer_id,
            ),
        };

        if forwarded.is_err() {
            shutdown_graceful(&quic, &by_quic);
            return;
        }
    }
}

fn shutdown_graceful(quic: &QuicHost, by_quic: &HashMap<u64, WtSession>) {
    for &qid in by_quic.keys() {
        quic.disconnect(qid, DisconnectReason::Graceful.code());
    }
}

const CHANNEL_RELIABLE: u8 = 0;
const CHANNEL_UNSEQUENCED: u8 = 2;

type Forward = Result<(), ()>;

fn send_event(events_tx: &UnboundedSender<Event>, event: Event) -> Forward {
    events_tx.send(event).map_err(|_| ())
}

#[allow(clippy::too_many_arguments)]
fn handle_connect(
    quic: &mut QuicHost,
    config: &WtConfig,
    metrics: &WtMetrics,
    by_quic: &mut HashMap<u64, WtSession>,
    quic_by_peer: &mut HashMap<u32, u64>,
    free_slots: &mut Vec<u32>,
    events_tx: &UnboundedSender<Event>,
    peer_id: u64,
    remote_addr: String,
) -> Forward {
    let Some(slot) = free_slots.pop() else {
        metrics.peers_refused_full.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(%remote_addr, "webtransport: peer pool exhausted — refusing");
        quic.disconnect(peer_id, config.server_full_reason);
        return Ok(());
    };
    by_quic.insert(
        peer_id,
        WtSession {
            our_peer: slot,
            reader: StreamFrameReader::new(config.max_message_bytes),
            deduper: DatagramDeduper::default(),
        },
    );
    quic_by_peer.insert(slot, peer_id);
    tracing::debug!(peer = slot, %remote_addr, "webtransport peer connected");
    send_event(
        events_tx,
        Event::Connect {
            peer: slot as PeerId,
            ip: Some(parse_ip(&remote_addr)),
        },
    )
}

fn handle_stream(
    by_quic: &mut HashMap<u64, WtSession>,
    events_tx: &UnboundedSender<Event>,
    peer_id: u64,
    data: &[u8],
) -> Forward {
    let Some(session) = by_quic.get_mut(&peer_id) else {
        return Ok(());
    };
    let peer = session.our_peer;
    session.reader.append(data);
    loop {
        match session.reader.try_read() {
            Ok(Some(msg)) => {
                send_event(
                    events_tx,
                    Event::Receive {
                        peer: peer as PeerId,
                        channel: CHANNEL_RELIABLE,
                        packet: Packet::reliable(CHANNEL_RELIABLE, msg),
                    },
                )?;
            }
            Ok(None) => return Ok(()),
            Err(_overrun) => {
                return send_event(
                    events_tx,
                    Event::Corrupt {
                        peer: peer as PeerId,
                    },
                );
            }
        }
    }
}

fn handle_datagram(
    metrics: &WtMetrics,
    by_quic: &mut HashMap<u64, WtSession>,
    events_tx: &UnboundedSender<Event>,
    peer_id: u64,
    data: &[u8],
) -> Forward {
    let Some(session) = by_quic.get_mut(&peer_id) else {
        return Ok(());
    };
    let peer = session.our_peer;
    let Some((channel_id, seq, payload)) = parse_datagram(data) else {
        return send_event(
            events_tx,
            Event::Corrupt {
                peer: peer as PeerId,
            },
        );
    };
    if channel_id == CHANNEL_SEQUENCED && !session.deduper.should_accept(channel_id, seq) {
        metrics
            .datagrams_dropped_stale
            .fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }
    let packet = if channel_id == CHANNEL_UNSEQUENCED {
        Packet::unsequenced(channel_id, payload.to_vec())
    } else {
        Packet::unreliable(channel_id, payload.to_vec())
    };
    send_event(
        events_tx,
        Event::Receive {
            peer: peer as PeerId,
            channel: channel_id,
            packet,
        },
    )
}

fn handle_disconnect(
    by_quic: &mut HashMap<u64, WtSession>,
    quic_by_peer: &mut HashMap<u32, u64>,
    free_slots: &mut Vec<u32>,
    events_tx: &UnboundedSender<Event>,
    peer_id: u64,
) -> Forward {
    let Some(session) = by_quic.remove(&peer_id) else {
        return Ok(());
    };
    quic_by_peer.remove(&session.our_peer);
    free_slots.push(session.our_peer);
    tracing::debug!(peer = session.our_peer, "webtransport peer disconnected");
    send_event(
        events_tx,
        Event::Disconnect {
            peer: session.our_peer as PeerId,
        },
    )
}

fn parse_ip(remote_addr: &str) -> String {
    remote_addr
        .parse::<SocketAddr>()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| remote_addr.to_string())
}
