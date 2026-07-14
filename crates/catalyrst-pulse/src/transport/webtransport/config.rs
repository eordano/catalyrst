use std::net::SocketAddr;

pub const DEFAULT_MAX_DATAGRAM_BYTES: usize = 1200;

pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 4096;

pub const DEFAULT_SERVICE_TIMEOUT_MS: u64 = 1;

#[derive(Debug, Clone)]
pub struct WtConfig {
    pub bind_addr: SocketAddr,
    pub cert_pem: String,
    pub key_pem: String,
    pub slot_base: u32,
    pub slot_capacity: usize,
    pub max_datagram_bytes: usize,
    pub max_message_bytes: usize,
    pub service_timeout_ms: u64,
    pub server_full_reason: u32,
}
