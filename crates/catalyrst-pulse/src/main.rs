use catalyrst_pulse::hardening::{
    DisconnectReason, GameplayRateLimiter, DEFAULT_DISCRETE_BURST, DEFAULT_DISCRETE_RATE_PER_SEC,
    DEFAULT_INPUT_BURST, DEFAULT_INPUT_MAX_HZ,
};
use catalyrst_pulse::server::{ENET_CAPACITY, WT_CAPACITY};
use catalyrst_pulse::transport::webtransport::config::{
    DEFAULT_MAX_DATAGRAM_BYTES, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_SERVICE_TIMEOUT_MS,
};
use catalyrst_pulse::transport::webtransport::WtConfig;
use catalyrst_pulse::PulseServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let bind = std::env::var("PULSE_BIND")
        .unwrap_or_else(|_| "0.0.0.0:9000".to_string())
        .parse()?;
    let wt = webtransport_config_from_env()?;
    let mut server = PulseServer::new();
    server.gameplay_limiter = GameplayRateLimiter::new(
        env_u32("PULSE_INPUT_MAX_HZ", DEFAULT_INPUT_MAX_HZ),
        env_u32("PULSE_INPUT_BURST", DEFAULT_INPUT_BURST),
        env_u32("PULSE_DISCRETE_RATE_PER_SEC", DEFAULT_DISCRETE_RATE_PER_SEC),
        env_u32("PULSE_DISCRETE_BURST", DEFAULT_DISCRETE_BURST),
    );
    server.run_with_webtransport(bind, 50, wt).await
}

/// Build the WebTransport config from the environment, or `None` when disabled. WebTransport is
/// off unless `PULSE_WT_ENABLED` is truthy; when on, it needs a certificate + key (inline PEM or
/// a file path) — a browser cannot reach a raw ENet/UDP socket, so this is the browser front door.
fn webtransport_config_from_env() -> anyhow::Result<Option<WtConfig>> {
    if !env_bool("PULSE_WT_ENABLED") {
        return Ok(None);
    }

    let bind_addr = std::env::var("PULSE_WT_BIND")
        .unwrap_or_else(|_| "0.0.0.0:7743".to_string())
        .parse()
        .map_err(|e| anyhow::anyhow!("PULSE_WT_BIND: {e}"))?;

    let cert_pem = read_pem("PULSE_WT_CERT_PEM", "PULSE_WT_CERT_PATH")?
        .ok_or_else(|| anyhow::anyhow!("PULSE_WT_ENABLED but no PULSE_WT_CERT_PEM/PATH set"))?;
    let key_pem = read_pem("PULSE_WT_KEY_PEM", "PULSE_WT_KEY_PATH")?
        .ok_or_else(|| anyhow::anyhow!("PULSE_WT_ENABLED but no PULSE_WT_KEY_PEM/PATH set"))?;

    Ok(Some(WtConfig {
        bind_addr,
        cert_pem,
        key_pem,
        slot_base: ENET_CAPACITY as u32,
        slot_capacity: WT_CAPACITY,
        max_datagram_bytes: env_usize("PULSE_WT_MAX_DATAGRAM_BYTES", DEFAULT_MAX_DATAGRAM_BYTES),
        max_message_bytes: env_usize("PULSE_WT_MAX_MESSAGE_BYTES", DEFAULT_MAX_MESSAGE_BYTES),
        service_timeout_ms: DEFAULT_SERVICE_TIMEOUT_MS,
        server_full_reason: DisconnectReason::ServerFull.code(),
    }))
}

fn env_bool(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "on")
    )
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Read a PEM blob from an inline env var (takes precedence) or a file path.
fn read_pem(inline_key: &str, path_key: &str) -> anyhow::Result<Option<String>> {
    if let Ok(pem) = std::env::var(inline_key) {
        if !pem.trim().is_empty() {
            return Ok(Some(pem));
        }
    }
    if let Ok(path) = std::env::var(path_key) {
        if !path.trim().is_empty() {
            return Ok(Some(
                std::fs::read_to_string(&path)
                    .map_err(|e| anyhow::anyhow!("reading {path_key}={path}: {e}"))?,
            ));
        }
    }
    Ok(None)
}
