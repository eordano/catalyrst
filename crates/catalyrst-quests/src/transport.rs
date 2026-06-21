//! axum WebSocket -> dcl-rpc transport adapter. Binary frames carry the
//! length-prefixed dcl-rpc protobuf packets; the per-connection authenticated
//! address is carried alongside for the service to key on. Mirrors
//! catalyrst-social-rpc's `AxumWsTransport`.

use axum::extract::ws::{Message, WebSocket};
use dcl_rpc::transports::{Transport, TransportError, TransportMessage};
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{Mutex, Notify};

pub struct AxumWsTransport {
    inbound_rx: Mutex<UnboundedReceiver<TransportMessage>>,
    outbound_tx: UnboundedSender<TransportMessage>,
    closed: AtomicBool,
    kill: Arc<Notify>,
    address: String,
}

impl AxumWsTransport {
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn spawn(socket: WebSocket, address: String) -> Self {
        let (mut sink, mut stream) = socket.split();
        let (inbound_tx, inbound_rx) = unbounded_channel::<TransportMessage>();
        let (outbound_tx, mut outbound_rx) = unbounded_channel::<TransportMessage>();
        let kill = Arc::new(Notify::new());

        let reader_kill = kill.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = reader_kill.notified() => break,
                    msg = stream.next() => {
                        match msg {
                            Some(Ok(Message::Binary(b))) => {
                                if inbound_tx.send(b.to_vec()).is_err() {
                                    break;
                                }
                            }
                            Some(Ok(Message::Text(_))) => {}
                            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        }
                    }
                }
            }
        });

        let writer_kill = kill.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = writer_kill.notified() => break,
                    bytes = outbound_rx.recv() => {
                        match bytes {
                            Some(bytes) => {
                                if sink.send(Message::Binary(bytes.into())).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
            let _ = sink.close().await;
        });

        Self {
            inbound_rx: Mutex::new(inbound_rx),
            outbound_tx,
            closed: AtomicBool::new(false),
            kill,
            address,
        }
    }
}

#[async_trait::async_trait]
impl Transport for AxumWsTransport {
    async fn receive(&self) -> Result<TransportMessage, TransportError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(TransportError::Closed);
        }
        let mut rx = self.inbound_rx.lock().await;
        let bytes = tokio::select! {
            _ = self.kill.notified() => None,
            recv = rx.recv() => recv,
        };
        match bytes {
            Some(bytes) => Ok(bytes),
            None => {
                self.closed.store(true, Ordering::Release);
                Err(TransportError::Closed)
            }
        }
    }

    async fn send(&self, message: TransportMessage) -> Result<(), TransportError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(TransportError::Closed);
        }
        self.outbound_tx
            .send(message)
            .map_err(|_| TransportError::Closed)
    }

    async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.kill.notify_waiters();
    }
}
