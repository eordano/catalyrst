//! In-process transport over [`async_channel`]: used for tests here, and on Decentraland for
//! the `Scenes<>BrowserInterface<>GameEngine` comms.
use super::{Transport, TransportError, TransportMessage};
use async_channel::{bounded, Receiver, Sender};
use async_trait::async_trait;

pub struct MemoryTransport {
    sender: Sender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
}

impl MemoryTransport {
    fn new(sender: Sender<Vec<u8>>, receiver: Receiver<Vec<u8>>) -> Self {
        Self { sender, receiver }
    }

    /// Returns the [`RpcClient`](crate::client::RpcClient) end first, the
    /// [`RpcServer`](crate::server::RpcServer) end second.
    pub fn create() -> (Self, Self) {
        let (client_sender, server_receiver) = bounded::<Vec<u8>>(32);
        let (server_sender, client_receiver) = bounded::<Vec<u8>>(32);

        let client = Self::new(client_sender, client_receiver);
        let server = Self::new(server_sender, server_receiver);

        (client, server)
    }
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn receive(&self) -> Result<TransportMessage, TransportError> {
        match self.receiver.recv().await {
            Ok(message) => Ok(message),
            Err(_) => {
                self.close().await;
                Err(TransportError::Closed)
            }
        }
    }

    async fn send(&self, message: Vec<u8>) -> Result<(), TransportError> {
        match self.sender.send(message).await {
            Ok(_) => Ok(()),
            Err(_) => Err(TransportError::Closed),
        }
    }

    async fn close(&self) {
        self.receiver.close();
    }
}
