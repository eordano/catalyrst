use catalyrst_pulse::hardening::{
    DisconnectReason, GameplayRateLimiter, DEFAULT_DISCRETE_BURST, DEFAULT_DISCRETE_RATE_PER_SEC,
    DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ,
};
use catalyrst_pulse::server::{ENET_CAPACITY, WT_CAPACITY};
use catalyrst_pulse::transport::webtransport::config::{
    DEFAULT_MAX_DATAGRAM_BYTES, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_SERVICE_TIMEOUT_MS,
};
use catalyrst_pulse::transport::webtransport::{WtConfig, WtHost};
use catalyrst_pulse::transport::{Host, HostConfig, Transports};
use catalyrst_pulse::PulseServer;

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind = std::env::var("PULSE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:9000".to_string())
        .parse()?;
    let wt_bind = std::env::var("PULSE_WT_BIND")
        .unwrap_or_else(|_| "127.0.0.1:7743".to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("PULSE_WT_BIND: {e}"))?;
    let cert_pem = std::fs::read_to_string(std::env::var("PULSE_WT_CERT_PATH")?)?;
    let key_pem = std::fs::read_to_string(std::env::var("PULSE_WT_KEY_PATH")?)?;

    let (wt, events) = WtHost::start(WtConfig {
        bind_addr: wt_bind,
        cert_pem,
        key_pem,
        slot_base: ENET_CAPACITY as u32,
        slot_capacity: WT_CAPACITY,
        max_datagram_bytes: DEFAULT_MAX_DATAGRAM_BYTES,
        max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
        service_timeout_ms: DEFAULT_SERVICE_TIMEOUT_MS,
        server_full_reason: DisconnectReason::ServerFull.code(),
    })?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let enet = Host::bind(HostConfig {
            bind,
            max_peers: ENET_CAPACITY,
            channel_limit: 8,
        })
        .await?;
        let transports = Transports::with_webtransport(enet, ENET_CAPACITY as u32, wt, events);
        let mut server = PulseServer::new();
        server.gameplay_limiter = GameplayRateLimiter::new(
            env_u32("PULSE_INPUT_MAX_HZ", DEFAULT_INPUT_MAX_HZ),
            env_u32("PULSE_INPUT_BURST", DEFAULT_INPUT_BURST),
            env_u32("PULSE_DISCRETE_RATE_PER_SEC", DEFAULT_DISCRETE_RATE_PER_SEC),
            env_u32("PULSE_DISCRETE_BURST", DEFAULT_DISCRETE_BURST),
        );
        server.serve(transports, 50).await
    })
}
