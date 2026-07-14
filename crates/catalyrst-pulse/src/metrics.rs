//! Scene-listener metrics over the `metrics` facade (exporter-agnostic; a no-op recorder when
//! none is installed, so these are safe to call from tests).

use std::net::SocketAddr;

use metrics::{counter, gauge, histogram};

pub const DEFAULT_METRICS_BIND: &str = "127.0.0.1:5005";

/// Install the Prometheus recorder and serve `/metrics` on `bind`.
///
/// Without it the facade below records into a no-op and the scrape target is dead, which
/// pins `up{job="pulse"}` at 0 and burns the shared `ServiceDown` alert for every other job
/// in its regex. A bind failure is therefore fatal rather than a warning: a metrics endpoint
/// that is silently absent trains the operator to ignore the alert.
pub fn install_prometheus_exporter(bind: SocketAddr) -> anyhow::Result<()> {
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(bind)
        .install()
        .map_err(|e| anyhow::anyhow!("failed to install prometheus exporter on {bind}: {e}"))
}

pub fn metrics_bind_from_env() -> anyhow::Result<SocketAddr> {
    let raw =
        std::env::var("PULSE_METRICS_BIND").unwrap_or_else(|_| DEFAULT_METRICS_BIND.to_string());
    raw.parse()
        .map_err(|e| anyhow::anyhow!("PULSE_METRICS_BIND `{raw}`: {e}"))
}

const CONNECTED: &str = "pulse_scene_listener_connected";
const FORBIDDEN_DROPPED: &str = "pulse_scene_listener_forbidden_messages_dropped_total";
const VISIBLE_SUBJECTS: &str = "pulse_scene_listener_visible_subjects";
const PARCELS: &str = "pulse_scene_listener_parcels";

pub fn scene_listener_connected_inc() {
    gauge!(CONNECTED).increment(1.0);
}

pub fn scene_listener_connected_dec() {
    gauge!(CONNECTED).decrement(1.0);
}

pub fn scene_listener_forbidden_dropped() {
    counter!(FORBIDDEN_DROPPED).increment(1);
}

pub fn scene_listener_visible_subjects(n: usize) {
    histogram!(VISIBLE_SUBJECTS).record(n as f64);
}

pub fn scene_listener_parcels(n: usize) {
    histogram!(PARCELS).record(n as f64);
}
