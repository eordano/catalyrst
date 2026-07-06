use std::path::PathBuf;
use std::time::{Duration, Instant};

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_CATALYST: &str = "https://peer.decentraland.org";

fn env_or(name: &str, default: String) -> String {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => v,
        _ => default,
    }
}

fn free_port() -> Option<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).ok()?;
    Some(l.local_addr().ok()?.port())
}

pub struct Sidecar {
    pub url: String,
    pub bin: String,
    _child: tokio::process::Child,
}

pub fn spawn_sidecar(preview_port: u16) -> Option<Sidecar> {
    let bin = env_or("ABGEN_BIN", "abgen".to_string());
    let port = free_port()?;
    let url = format!("http://127.0.0.1:{port}");
    let cache_root: PathBuf = std::env::temp_dir().join("dcl-abgen");

    let spawned = tokio::process::Command::new(&bin)
        .env("HTTP_SERVER_HOST", "127.0.0.1")
        .env("HTTP_SERVER_PORT", port.to_string())
        .env(
            "ABGEN_CATALYST_URL",
            env_or(
                "ABGEN_CATALYST_URL",
                format!("http://127.0.0.1:{preview_port}/content"),
            ),
        )
        .env(
            "ABGEN_WORLDS_CONTENT_URL",
            env_or(
                "ABGEN_WORLDS_CONTENT_URL",
                format!("{DEFAULT_CATALYST}/content"),
            ),
        )
        .env(
            "ABGEN_OUT_ROOT",
            env_or(
                "ABGEN_OUT_ROOT",
                cache_root.join("out").display().to_string(),
            ),
        )
        .env(
            "ABGEN_CACHE_DIR",
            env_or(
                "ABGEN_CACHE_DIR",
                cache_root.join("cache").display().to_string(),
            ),
        )
        .env(
            "RUST_LOG",
            env_or("RUST_LOG", "abgen=info,tower_http=warn".to_string()),
        )
        .kill_on_drop(true)
        .spawn();

    match spawned {
        Ok(child) => Some(Sidecar {
            url,
            bin,
            _child: child,
        }),
        Err(e) => {
            crate::ux::note_stderr(format!(
                "warning: asset-bundles: {bin} failed to start ({})",
                e.kind()
            ));
            warn_not_up(&bin, &url);
            None
        }
    }
}

pub async fn wait_ready(url: &str, bin: &str) -> bool {
    let ready_url = format!("{url}/readyz");
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(res) = client.get(&ready_url).send().await {
            if res.status().is_success() {
                return true;
            }
        }
        tokio::time::sleep(READY_POLL_INTERVAL).await;
    }
    warn_not_up(bin, url);
    false
}

fn warn_not_up(bin: &str, url: &str) {
    crate::ux::note_stderr(format!(
        "warning: asset-bundles: {bin} did not come up on {url}. Install the abgen binary (or set ABGEN_BIN to its path) to serve asset bundles in preview, or pass --no-asset-bundles to turn this off."
    ));
}
