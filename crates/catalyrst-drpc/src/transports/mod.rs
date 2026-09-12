use async_trait::async_trait;

pub mod error;
#[cfg(feature = "memory")]
pub mod memory;
#[cfg(feature = "websockets")]
pub mod web_sockets;

pub type TransportMessage = Vec<u8>;

#[derive(Debug)]
pub enum TransportError {
    Internal(Box<dyn std::error::Error + Send + Sync>),
    Closed,
    NotBinaryMessage,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn receive(&self) -> Result<TransportMessage, TransportError>;
    async fn send(&self, message: TransportMessage) -> Result<(), TransportError>;
    async fn close(&self);
}
