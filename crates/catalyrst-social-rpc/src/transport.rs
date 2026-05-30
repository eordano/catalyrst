use axum::extract::ws::{Message, WebSocket};
use dcl_rpc::transports::{Transport, TransportError, TransportMessage};
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;

pub struct AxumWsTransport {
    inbound_rx: Mutex<UnboundedReceiver<TransportMessage>>,
    outbound_tx: UnboundedSender<TransportMessage>,
    closed: AtomicBool,

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

        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(Message::Binary(b)) => {
                        if inbound_tx.send(b.to_vec()).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Text(_)) => {
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                    Ok(Message::Close(_)) | Err(_) => break,
                }
            }
        });

        tokio::spawn(async move {
            while let Some(bytes) = outbound_rx.recv().await {
                if sink.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            let _ = sink.close().await;
        });

        Self {
            inbound_rx: Mutex::new(inbound_rx),
            outbound_tx,
            closed: AtomicBool::new(false),
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
        match self.inbound_rx.lock().await.recv().await {
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
    }
}
