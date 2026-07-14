use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Liveness of the SFU a realm advertises, so `/about` can stop pointing clients
/// at comms that will not answer.
///
/// This matters more than a degraded-comms flag suggests. The desktop client
/// treats the LiveKit handshake as a hard gate on entry
/// (`EnsureLivekitConnectionStartupOperation`): the handshake result overwrites
/// the scene-load result, so when it times out the loading screen never
/// completes and the visitor is thrown back to the login screen. Content that is
/// perfectly healthy is unreachable because comms is not. Advertising
/// `offline:offline` instead lets them in with comms off, and they get the real
/// thing on their next entry once the SFU answers.
///
/// Starts alive and needs [`STRIKES_TO_DOWN`] consecutive failures to flip. A
/// correctly configured node has a live SFU essentially always, so guessing
/// "alive" before the first probe returns keeps a worlds restart from silently
/// muting a healthy realm; a node whose SFU is genuinely gone corrects itself
/// within a few probe intervals and stays corrected.
const PROBE_INTERVAL: Duration = Duration::from_secs(10);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const STRIKES_TO_DOWN: u32 = 3;

#[derive(Clone)]
pub struct SfuHealth {
    alive: Arc<AtomicBool>,
    down_since_secs: Arc<AtomicU64>,
    target: Arc<str>,
    enabled: bool,
}

impl SfuHealth {
    /// No probing: liveness is always reported as alive. For deployments that
    /// opted out, and for tests.
    pub fn always_alive() -> Self {
        Self {
            alive: Arc::new(AtomicBool::new(true)),
            down_since_secs: Arc::new(AtomicU64::new(0)),
            target: Arc::from(""),
            enabled: false,
        }
    }

    /// Probes `target` (an http/https URL) on a background task for as long as
    /// the process lives.
    pub fn spawn(target: String) -> Self {
        let health = Self {
            alive: Arc::new(AtomicBool::new(true)),
            down_since_secs: Arc::new(AtomicU64::new(0)),
            target: Arc::from(target.as_str()),
            enabled: true,
        };
        // Callers reach this through lazily-initialised config as often as from
        // a startup path, so an absent runtime is a caller shape, not a bug: it
        // degrades to never probing rather than panicking inside a request.
        if tokio::runtime::Handle::try_current().is_err() {
            tracing::warn!(
                target = %target,
                "SFU health probe started outside a Tokio runtime; comms will always be advertised as up"
            );
            return Self::always_alive();
        }
        let probe = health.clone();
        tokio::spawn(async move { probe.run(target).await });
        health
    }

    pub fn is_alive(&self) -> bool {
        !self.enabled || self.alive.load(Ordering::Relaxed)
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    /// Seconds the SFU has been unreachable, or `None` while it is answering.
    pub fn down_for_secs(&self) -> Option<u64> {
        if self.is_alive() {
            return None;
        }
        let since = self.down_since_secs.load(Ordering::Relaxed);
        Some(now_secs().saturating_sub(since))
    }

    async fn run(self, target: String) {
        let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "SFU health probe could not build an HTTP client; comms will always be advertised as up"
                );
                return;
            }
        };
        let mut strikes: u32 = 0;
        loop {
            // Any HTTP answer proves the process is listening, which is what the
            // client's handshake needs; LiveKit's root path has no health
            // contract beyond that, so a 404 counts as alive.
            let reachable = client.get(&target).send().await.is_ok();
            if reachable {
                strikes = 0;
                if !self.alive.swap(true, Ordering::Relaxed) {
                    tracing::info!(target = %target, "SFU is answering again; comms restored");
                }
            } else {
                strikes = strikes.saturating_add(1);
                if strikes >= STRIKES_TO_DOWN && self.alive.swap(false, Ordering::Relaxed) {
                    self.down_since_secs.store(now_secs(), Ordering::Relaxed);
                    tracing::warn!(
                        target = %target,
                        strikes,
                        "SFU unreachable: /about now advertises fixed-adapter:offline:offline so visitors can still enter, with comms off"
                    );
                }
            }
            tokio::time::sleep(PROBE_INTERVAL).await;
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The HTTP address to probe for a client-facing comms endpoint.
///
/// Accepts what the deployment already configures: a LiveKit signalling URL
/// (`wss://host`, `ws://host:7880`), a bare host, or a `signed-login:`/
/// `fixed-adapter:` adapter string pointing at a gatekeeper. A gatekeeper that
/// does not answer bounces entry exactly like a dead SFU, so probing whichever
/// of the two the realm actually advertises is the honest check.
pub fn probe_target(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("fixed-adapter:")
        .unwrap_or(raw)
        .trim_start_matches("signed-login:")
        .trim();
    if raw.is_empty() || raw == "offline:offline" {
        return None;
    }
    let url = match raw.split_once("://") {
        Some(("wss", rest)) => format!("https://{rest}"),
        Some(("ws", rest)) => format!("http://{rest}"),
        Some(("https", rest)) => format!("https://{rest}"),
        Some(("http", rest)) => format!("http://{rest}"),
        Some(_) => return None,
        None => format!("https://{raw}"),
    };
    Some(url.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_signalling_schemes_onto_http() {
        assert_eq!(
            probe_target("wss://livekit.example.org").as_deref(),
            Some("https://livekit.example.org")
        );
        assert_eq!(
            probe_target("ws://127.0.0.1:7880").as_deref(),
            Some("http://127.0.0.1:7880")
        );
    }

    #[test]
    fn accepts_a_bare_host_and_a_host_with_a_path_prefix() {
        assert_eq!(
            probe_target("livekit.local").as_deref(),
            Some("https://livekit.local")
        );
        assert_eq!(
            probe_target("example.org/livekit/").as_deref(),
            Some("https://example.org/livekit")
        );
    }

    #[test]
    fn unwraps_an_adapter_string_down_to_the_gatekeeper() {
        assert_eq!(
            probe_target(
                "fixed-adapter:signed-login:https://realm.example.org/comms/get-scene-adapter"
            )
            .as_deref(),
            Some("https://realm.example.org/comms/get-scene-adapter")
        );
    }

    #[test]
    fn nothing_to_probe_when_comms_is_already_offline() {
        assert!(probe_target("fixed-adapter:offline:offline").is_none());
        assert!(probe_target("   ").is_none());
    }

    #[test]
    fn opted_out_health_always_reports_alive() {
        let h = SfuHealth::always_alive();
        assert!(h.is_alive());
        assert!(h.down_for_secs().is_none());
    }
}
