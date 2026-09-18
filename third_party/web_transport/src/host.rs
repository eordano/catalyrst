//! The WebTransport host: binds a QUIC endpoint, accepts WebTransport sessions, and pumps
//! the reliable bidi stream and unreliable datagrams to/from a synchronously-polled event
//! queue. Mirrors ENet's `enet_host_service` model so the .NET threading model is unchanged.
//!
//! This crate is a *generic* byte pipe: it does no framing or sequencing. Pulse channel
//! semantics (length framing on the stream, datagram sequencing/dedup) live in the C# layer.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use web_transport_quinn::{Request, Session};

/// Buffer size for a single reliable-stream read. Raw chunks are forwarded as-is.
const STREAM_READ_BUF: usize = 16 * 1024;
const STREAM_SEND_WINDOW: u64 = 64 * 1024;

/// Settings for constructing a [`Host`].
pub struct HostConfig {
    /// UDP socket address to bind the QUIC endpoint to. Use port `0` to let the OS choose.
    pub bind_addr: SocketAddr,
    /// PEM-encoded certificate chain.
    pub cert_pem: String,
    /// PEM-encoded PKCS#8 / SEC1 private key.
    pub key_pem: String,
}

/// Errors returned by [`Host::new`].
#[derive(Debug)]
pub enum HostError {
    /// The certificate or key PEM could not be parsed.
    Cert(String),
    /// The QUIC endpoint could not be created or bound.
    Endpoint(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Cert(m) => write!(f, "certificate error: {m}"),
            HostError::Endpoint(m) => write!(f, "endpoint error: {m}"),
        }
    }
}

impl std::error::Error for HostError {}

/// An event drained from the host, mirroring ENet's `enet_host_service` event model.
#[derive(Debug, Clone)]
pub enum Event {
    /// A new WebTransport session was established. `remote_addr` is the peer's `ip:port`.
    Connect { peer_id: u64, remote_addr: String },
    /// A session ended (peer closed, transport error, or local disconnect).
    Disconnect { peer_id: u64, reason: u32 },
    /// Raw bytes arrived on the peer's reliable bidi stream (unframed).
    StreamData { peer_id: u64, data: Bytes },
    /// An unreliable datagram arrived from the peer.
    Datagram { peer_id: u64, data: Bytes },
}

/// Commands sent from the polling thread into a connection's async task.
enum PeerCommand {
    /// Write raw bytes to the peer's reliable bidi stream.
    Stream(Bytes),
}

/// Per-connection state retained by the host so the polling thread can send to a peer.
struct PeerHandle {
    /// A clone of the session, used for the sync send-datagram / rtt / close paths.
    session: Session,
    /// Channel into the connection's writer task for the async reliable-stream path.
    cmd_tx: UnboundedSender<PeerCommand>,
}

/// A running WebTransport server. Construct with [`Host::new`], then drive it by repeatedly
/// calling [`Host::service`] from a single dedicated thread (the ENet model). The other
/// methods may be called from that same thread between `service` calls.
pub struct Host {
    runtime: Option<Runtime>,
    events_rx: Receiver<Event>,
    peers: Arc<Mutex<HashMap<u64, PeerHandle>>>,
    local_addr: SocketAddr,
}

impl Host {
    /// Bind the QUIC endpoint, start the tokio runtime, and begin accepting sessions.
    pub fn new(config: HostConfig) -> Result<Host, HostError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| HostError::Endpoint(e.to_string()))?;

        let (certs, key) = parse_pem(&config.cert_pem, &config.key_pem)?;
        let bind_addr = config.bind_addr;

        // `with_certificate` constructs a quinn endpoint, which needs a runtime context.
        let server = runtime
            .block_on(async move {
                web_transport_quinn::ServerBuilder::new()
                    .with_addr(bind_addr)
                    .with_certificate(certs, key)
            })
            .map_err(|e| HostError::Endpoint(e.to_string()))?;

        let local_addr = server
            .local_addr()
            .map_err(|e| HostError::Endpoint(e.to_string()))?;

        let (events_tx, events_rx) = std::sync::mpsc::channel::<Event>();
        let peers: Arc<Mutex<HashMap<u64, PeerHandle>>> = Arc::new(Mutex::new(HashMap::new()));
        let accept_peers = peers.clone();

        // Accept loop: assign sequential peer ids and serve each session on its own task.
        runtime.spawn(async move {
            let mut server = server;
            let mut next_id: u64 = 1;
            while let Some(request) = server.accept().await {
                let peer_id = next_id;
                next_id += 1;
                tokio::spawn(serve_connection(
                    request,
                    peer_id,
                    events_tx.clone(),
                    accept_peers.clone(),
                ));
            }
        });

        Ok(Host {
            runtime: Some(runtime),
            events_rx,
            peers,
            local_addr,
        })
    }

    /// The address the endpoint is bound to, with the OS-assigned port resolved.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Block up to `timeout` for the next event, mirroring `enet_host_service`.
    /// Returns `None` if no event arrived within `timeout`.
    pub fn service(&mut self, timeout: Duration) -> Option<Event> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    /// Queue raw bytes for the peer's reliable bidi stream. Returns `false` if the peer is gone.
    pub fn send_stream(&self, peer_id: u64, data: &[u8]) -> bool {
        self.send_stream_bytes(peer_id, Bytes::copy_from_slice(data))
    }

    /// Retains the byte owner's allocation and queue permits until the stream write completes.
    pub fn send_stream_bytes(&self, peer_id: u64, data: Bytes) -> bool {
        let peers = self
            .peers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match peers.get(&peer_id) {
            Some(handle) => handle
                .cmd_tx
                .send(PeerCommand::Stream(data))
                .is_ok(),
            None => false,
        }
    }

    /// Send an unreliable datagram to the peer. Returns `false` on failure (gone / too large).
    pub fn send_datagram(&self, peer_id: u64, data: &[u8]) -> bool {
        let peers = self
            .peers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match peers.get(&peer_id) {
            Some(handle) => handle
                .session
                .send_datagram(Bytes::copy_from_slice(data))
                .is_ok(),
            None => false,
        }
    }

    /// Close the peer's session with an application error code. Returns `false` if gone.
    pub fn disconnect(&self, peer_id: u64, reason: u32) -> bool {
        let peers = self
            .peers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match peers.get(&peer_id) {
            Some(handle) => {
                // `Session::close` must run inside the tokio runtime context.
                if let Some(runtime) = self.runtime.as_ref() {
                    let _guard = runtime.enter();
                    handle.session.close(reason, b"");
                }
                true
            }
            None => false,
        }
    }

    /// Current smoothed RTT to the peer in microseconds, if still connected.
    pub fn rtt_us(&self, peer_id: u64) -> Option<u64> {
        let peers = self
            .peers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        peers
            .get(&peer_id)
            .map(|handle| handle.session.rtt().as_micros() as u64)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            // Don't block teardown indefinitely on in-flight connection tasks.
            runtime.shutdown_timeout(Duration::from_millis(200));
        }
    }
}

/// Establish one session, publish its lifecycle/data as [`Event`]s, and pump it until close.
async fn serve_connection(
    request: Request,
    peer_id: u64,
    events_tx: Sender<Event>,
    peers: Arc<Mutex<HashMap<u64, PeerHandle>>>,
) {
    let session = match request.ok().await {
        Ok(session) => session,
        Err(_) => return,
    };
    session.set_send_window(STREAM_SEND_WINDOW);
    let remote_addr = session.remote_address().to_string();

    let (cmd_tx, cmd_rx) = unbounded_channel::<PeerCommand>();
    peers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            peer_id,
            PeerHandle {
                session: session.clone(),
                cmd_tx,
            },
        );
    let _ = events_tx.send(Event::Connect {
        peer_id,
        remote_addr,
    });

    // Unreliable datagrams in.
    tokio::spawn(datagram_loop(session.clone(), peer_id, events_tx.clone()));
    // Reliable bidi stream: the peer opens it; we forward raw chunks and write queued bytes.
    tokio::spawn(stream_loop(
        session.clone(),
        peer_id,
        events_tx.clone(),
        cmd_rx,
    ));

    // This task is the disconnect monitor.
    let _ = session.closed().await;
    let _ = events_tx.send(Event::Disconnect { peer_id, reason: 0 });
    peers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&peer_id);
}

/// Forward inbound datagrams as [`Event::Datagram`] until the session closes.
async fn datagram_loop(session: Session, peer_id: u64, events_tx: Sender<Event>) {
    while let Ok(data) = session.read_datagram().await {
        if events_tx.send(Event::Datagram { peer_id, data }).is_err() {
            break;
        }
    }
}

/// Accept the peer's reliable bidi stream, forward inbound chunks as [`Event::StreamData`],
/// and write outbound [`PeerCommand::Stream`] bytes.
async fn stream_loop(
    session: Session,
    peer_id: u64,
    events_tx: Sender<Event>,
    mut cmd_rx: UnboundedReceiver<PeerCommand>,
) {
    let (mut send, mut recv) = match tokio::select! {
        stream = session.accept_bi() => stream,
        _ = session.closed() => return,
    } {
        Ok(pair) => pair,
        Err(_) => return,
    };

    let reader = tokio::spawn(async move {
        let mut buf = vec![0u8; STREAM_READ_BUF];
        while let Ok(Some(n)) = recv.read(&mut buf).await {
            let data = Bytes::copy_from_slice(&buf[..n]);
            if events_tx.send(Event::StreamData { peer_id, data }).is_err() {
                break;
            }
        }
    });

    loop {
        let command = tokio::select! {
            command = cmd_rx.recv() => command,
            _ = session.closed() => break,
        };
        let Some(PeerCommand::Stream(bytes)) = command else { break };
        let written = tokio::select! {
            result = send.write_all(&bytes) => result.is_ok(),
            _ = session.closed() => false,
        };
        if !written {
            session.close(0, b"stream write failed");
            break;
        }
    }

    reader.abort();
}

/// Parse a PEM certificate chain and private key into the rustls types
/// `web-transport-quinn` expects.
fn parse_pem(
    cert_pem: &str,
    key_pem: &str,
) -> Result<
    (
        Vec<rustls_pki_types::CertificateDer<'static>>,
        rustls_pki_types::PrivateKeyDer<'static>,
    ),
    HostError,
> {
    let mut cert_rd = std::io::BufReader::new(cert_pem.as_bytes());
    let certs = rustls_pemfile::certs(&mut cert_rd)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| HostError::Cert(e.to_string()))?;
    if certs.is_empty() {
        return Err(HostError::Cert("no certificates found in PEM".into()));
    }

    let mut key_rd = std::io::BufReader::new(key_pem.as_bytes());
    let key = rustls_pemfile::private_key(&mut key_rd)
        .map_err(|e| HostError::Cert(e.to_string()))?
        .ok_or_else(|| HostError::Cert("no private key found in PEM".into()))?;

    Ok((certs, key))
}
