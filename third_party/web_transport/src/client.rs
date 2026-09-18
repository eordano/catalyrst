//! WebTransport client: connects to a server, opens the reliable bidi stream, and pumps
//! datagrams + stream bytes to/from a synchronously-polled event queue. A single session.
//!
//! Primarily for the .NET test client (`DCLPulseTestClient`) to exercise the Pulse WebTransport
//! transport over loopback. Like [`crate::host::Host`], it is a generic byte pipe driven by
//! repeated [`Client::service`] calls; framing/sequencing live in the C# layer.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use bytes::Bytes;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use web_transport_quinn::Session;

/// Buffer size for a single reliable-stream read. Raw chunks are forwarded as-is.
const STREAM_READ_BUF: usize = 16 * 1024;

/// Settings for [`Client::connect`].
pub struct ClientConfig {
    /// Server URL, e.g. `"https://127.0.0.1:4433/"`.
    pub url: String,
    /// SHA-256 of the server's certificate (the WebTransport `serverCertificateHashes` trust
    /// path, for self-signed dev certs). `None` validates against the system root store.
    pub server_cert_hash: Option<Vec<u8>>,
}

/// Errors returned by [`Client::connect`].
#[derive(Debug)]
pub enum ClientError {
    /// The URL or trust configuration was invalid.
    Config(String),
    /// The connection (TLS / WebTransport handshake / stream open) failed.
    Connect(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Config(m) => write!(f, "config error: {m}"),
            ClientError::Connect(m) => write!(f, "connect error: {m}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// An event drained from the client. There is a single session, so no peer id.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// The session ended (server closed, transport error, or local disconnect).
    Disconnected { reason: u32 },
    /// Raw bytes arrived on the reliable bidi stream (unframed).
    StreamData { data: Bytes },
    /// An unreliable datagram arrived.
    Datagram { data: Bytes },
}

/// Command from the polling thread into the stream-writer task.
enum ClientCommand {
    Stream(Vec<u8>),
}

/// A connected WebTransport client. Construct with [`Client::connect`], then drive it by
/// repeatedly calling [`Client::service`] from a single dedicated thread.
pub struct Client {
    runtime: Option<Runtime>,
    events_rx: Receiver<ClientEvent>,
    session: Session,
    cmd_tx: UnboundedSender<ClientCommand>,
}

impl Client {
    /// Connect to the server, open the reliable bidi stream, and start pumping events.
    /// Blocks until the session is established (or fails).
    pub fn connect(config: ClientConfig) -> Result<Client, ClientError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| ClientError::Connect(e.to_string()))?;

        let url = url::Url::parse(&config.url).map_err(|e| ClientError::Config(e.to_string()))?;
        let cert_hash = config.server_cert_hash;

        let (session, send, recv) = runtime.block_on(async move {
            let builder = web_transport_quinn::ClientBuilder::new();
            let client = match cert_hash {
                Some(hash) => builder.with_server_certificate_hashes(vec![hash]),
                None => builder.with_system_roots(),
            }
            .map_err(|e| ClientError::Connect(e.to_string()))?;

            let session = client
                .connect(url)
                .await
                .map_err(|e| ClientError::Connect(e.to_string()))?;
            // Client opens the reliable bidi stream; the server accepts it (server `accept_bi`).
            let (send, recv) = session
                .open_bi()
                .await
                .map_err(|e| ClientError::Connect(e.to_string()))?;
            Ok::<_, ClientError>((session, send, recv))
        })?;

        let (events_tx, events_rx) = std::sync::mpsc::channel::<ClientEvent>();
        let (cmd_tx, cmd_rx) = unbounded_channel::<ClientCommand>();

        runtime.spawn(datagram_loop(session.clone(), events_tx.clone()));
        runtime.spawn(stream_reader(recv, events_tx.clone()));
        runtime.spawn(stream_writer(send, cmd_rx));
        runtime.spawn(disconnect_monitor(session.clone(), events_tx));

        Ok(Client {
            runtime: Some(runtime),
            events_rx,
            session,
            cmd_tx,
        })
    }

    /// Block up to `timeout` for the next event. Returns `None` on timeout.
    pub fn service(&mut self, timeout: Duration) -> Option<ClientEvent> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    /// Queue raw bytes for the reliable bidi stream. Returns `false` if the session is gone.
    pub fn send_stream(&self, data: &[u8]) -> bool {
        self.cmd_tx
            .send(ClientCommand::Stream(data.to_vec()))
            .is_ok()
    }

    /// Send an unreliable datagram. Returns `false` on failure (gone / too large).
    pub fn send_datagram(&self, data: &[u8]) -> bool {
        self.session
            .send_datagram(Bytes::copy_from_slice(data))
            .is_ok()
    }

    /// Close the session with an application error code.
    pub fn disconnect(&self, reason: u32) {
        // `Session::close` must run inside the tokio runtime context.
        if let Some(runtime) = self.runtime.as_ref() {
            let _guard = runtime.enter();
            self.session.close(reason, b"");
        }
    }

    /// Current smoothed RTT to the server in microseconds.
    pub fn rtt_us(&self) -> u64 {
        self.session.rtt().as_micros() as u64
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_millis(200));
        }
    }
}

async fn datagram_loop(session: Session, events_tx: Sender<ClientEvent>) {
    while let Ok(data) = session.read_datagram().await {
        if events_tx.send(ClientEvent::Datagram { data }).is_err() {
            break;
        }
    }
}

async fn stream_reader(mut recv: web_transport_quinn::RecvStream, events_tx: Sender<ClientEvent>) {
    let mut buf = vec![0u8; STREAM_READ_BUF];
    while let Ok(Some(n)) = recv.read(&mut buf).await {
        let data = Bytes::copy_from_slice(&buf[..n]);
        if events_tx.send(ClientEvent::StreamData { data }).is_err() {
            break;
        }
    }
}

async fn stream_writer(
    mut send: web_transport_quinn::SendStream,
    mut cmd_rx: UnboundedReceiver<ClientCommand>,
) {
    while let Some(ClientCommand::Stream(bytes)) = cmd_rx.recv().await {
        if send.write_all(&bytes).await.is_err() {
            break;
        }
    }
}

async fn disconnect_monitor(session: Session, events_tx: Sender<ClientEvent>) {
    let _ = session.closed().await;
    let _ = events_tx.send(ClientEvent::Disconnected { reason: 0 });
}
