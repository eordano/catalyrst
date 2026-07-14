use std::sync::OnceLock;

use catalyrst_livekit::SfuHealth;

/// Liveness of the comms endpoint this realm advertises in `/about`.
///
/// The desktop client gates entry on the LiveKit handshake rather than
/// degrading when it fails, so a realm that advertises comms nothing answers
/// does not lose voice -- it becomes unenterable, and the loading screen bounces
/// the visitor back to the login screen every 30 seconds. Serving
/// `offline:offline` while the endpoint is down trades multiplayer for a realm
/// people can walk into.
///
/// Probes `LIVEKIT_HOST` when set, and otherwise whatever `COMMS_FIXED_ADAPTER`
/// points at: a gatekeeper that cannot mint a token bounces entry exactly like a
/// dead SFU, so the honest check is against whichever endpoint clients are
/// actually sent to.
pub fn comms_health() -> &'static SfuHealth {
    static HEALTH: OnceLock<SfuHealth> = OnceLock::new();
    HEALTH.get_or_init(|| {
        if !catalyrst_envcfg::env_bool("COMMS_OFFLINE_WHEN_UNREACHABLE", true) {
            return SfuHealth::always_alive();
        }
        let configured = std::env::var("LIVEKIT_HOST")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("COMMS_FIXED_ADAPTER").ok())
            .unwrap_or_default();
        match catalyrst_livekit::probe_target(&configured) {
            Some(target) => SfuHealth::spawn(target),
            None => SfuHealth::always_alive(),
        }
    })
}
