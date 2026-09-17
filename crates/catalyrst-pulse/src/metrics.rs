//! Pulse server metrics over the `metrics` facade (exporter-agnostic; a no-op recorder when
//! none is installed, so these are safe to call from tests).
//!
//! Series carry the `pulse_` prefix, not upstream's `dcl_`; keep every new one on it so a single
//! exposition never needs two prefixes on one dashboard.

use std::net::SocketAddr;

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{BuildError, Matcher, PrometheusBuilder};

pub const DEFAULT_METRICS_BIND: &str = "127.0.0.1:5005";

/// Every builder that renders our histograms goes through here: a bucket override left off one
/// of them turns that metric into a summary, which cannot be aggregated across replicas.
pub fn prometheus_builder() -> Result<PrometheusBuilder, BuildError> {
    PrometheusBuilder::new().set_buckets_for_metric(
        Matcher::Full(RESYNC_SEQ_GAP.to_string()),
        &RESYNC_SEQ_GAP_BUCKETS,
    )
}

/// Without it the facade below records into a no-op and the scrape target is dead, which
/// pins `up{job="pulse"}` at 0 and burns the shared `ServiceDown` alert for every other job
/// in its regex. A bind failure is therefore fatal rather than a warning: a metrics endpoint
/// that is silently absent trains the operator to ignore the alert.
pub fn install_prometheus_exporter(bind: SocketAddr) -> anyhow::Result<()> {
    prometheus_builder()
        .and_then(|b| b.with_http_listener(bind).install())
        .map_err(|e| anyhow::anyhow!("failed to install prometheus exporter on {bind}: {e}"))?;
    register_resync_seq_gap_series();
    Ok(())
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
const RESYNC_SEQ_GAP: &str = "pulse_resync_seq_gap";

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

/// Tracks upstream Pulse `PulseMetrics.Simulation.RESYNC_SEQ_GAP_BUCKETS`; move it only in
/// lockstep. Dense around the snapshot ring depth, the eviction cliff a baseline crosses to turn
/// a targeted delta into a STATE_FULL, with a `0` edge separating "already current" from "one
/// publish behind" and a tail measuring how many publishes a client went dark for.
pub const RESYNC_SEQ_GAP_BUCKETS: [f64; 14] = [
    0.0, 1.0, 2.0, 4.0, 8.0, 10.0, 16.0, 20.0, 24.0, 32.0, 64.0, 128.0, 256.0, 1024.0,
];

pub const RESYNC_OUTCOME_DELTA: &str = "delta";
pub const RESYNC_OUTCOME_FULL: &str = "full";

/// Registering a labelled series costs one handle and makes it render at zero from process
/// start; do the same for any future labelled metric, or a share-of-total query reads `no data`
/// instead of `0` until the first sample and an alert on it never arms.
pub fn register_resync_seq_gap_series() {
    let _ = histogram!(RESYNC_SEQ_GAP, "outcome" => RESYNC_OUTCOME_DELTA);
    let _ = histogram!(RESYNC_SEQ_GAP, "outcome" => RESYNC_OUTCOME_FULL);
}

/// `last_known_seq` is unvalidated client input, so the ordering is tested rather than inferred
/// from the sign of a wrapped difference: a baseline at or ahead of the latest publish records 0,
/// never the ~4-billion gap a wrapping subtraction would put in `_sum` and the `+Inf` bucket.
pub fn resync_seq_gap(outcome: &'static str, latest_seq: u32, last_known_seq: u32) {
    histogram!(RESYNC_SEQ_GAP, "outcome" => outcome)
        .record(latest_seq.saturating_sub(last_known_seq) as f64);
}
