use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use catalyrst_pulse::batch::SeqEncoding;
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

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        System.alloc_zeroed(layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn spawn_alloc_writer(path: String) {
    std::thread::spawn(move || {
        let mut buf = String::with_capacity(48);
        loop {
            buf.clear();
            let _ = write!(
                buf,
                "{} {}",
                ALLOC_COUNT.load(Ordering::Relaxed),
                ALLOC_BYTES.load(Ordering::Relaxed)
            );
            let _ = std::fs::write(&path, &buf);
            std::thread::sleep(Duration::from_millis(200));
        }
    });
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
    let tick_ms: u64 = env_u32("PULSE_TICK_MS", 50) as u64;
    let seq_encoding = match std::env::var("PULSE_SEQ_ENCODING").as_deref() {
        Ok("delta") => SeqEncoding::Delta,
        _ => SeqEncoding::Absolute,
    };

    if let Ok(path) = std::env::var("PULSE_ALLOC_STAT_PATH") {
        spawn_alloc_writer(path);
    }

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
        server.simulation.set_seq_encoding(seq_encoding);
        server.gameplay_limiter = GameplayRateLimiter::new(
            env_u32("PULSE_INPUT_MAX_HZ", DEFAULT_INPUT_MAX_HZ),
            env_u32("PULSE_INPUT_BURST", DEFAULT_INPUT_BURST),
            env_u32("PULSE_DISCRETE_RATE_PER_SEC", DEFAULT_DISCRETE_RATE_PER_SEC),
            env_u32("PULSE_DISCRETE_BURST", DEFAULT_DISCRETE_BURST),
        );
        server.serve(transports, tick_ms).await
    })
}
