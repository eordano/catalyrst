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
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full(RESYNC_SEQ_GAP.to_string()),
            &RESYNC_SEQ_GAP_BUCKETS,
        )?
        .set_buckets_for_metric(
            Matcher::Full(CLUSTER_SIZE.to_string()),
            &CLUSTER_SIZE_BUCKETS,
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
    nats_connected(false);
    Ok(())
}

/// Renders what `f` recorded through a recorder local to this thread, so a series can be asserted
/// without installing a global one.
#[cfg(test)]
pub(crate) fn render_recorded<T>(f: impl FnOnce() -> T) -> String {
    let recorder = prometheus_builder()
        .expect("prometheus builder")
        .build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, f);
    handle.render()
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
const HANDSHAKES: &str = "pulse_handshakes_total";

const CLUSTER_COUNT: &str = "pulse_clusters";
const CLUSTER_PASSES: &str = "pulse_cluster_passes_total";
const CLUSTER_PASS_DURATION_US: &str = "pulse_cluster_pass_duration_us_total";
const CLUSTER_REASSIGNMENTS: &str = "pulse_cluster_reassignments_total";
const CLUSTER_TAKEOVERS: &str = "pulse_cluster_takeovers_total";
const CLUSTER_LOOKUPS: &str = "pulse_cluster_lookups_total";
const CLUSTER_LOOKUP_REPLIES: &str = "pulse_cluster_lookup_submitted_total";
const CLUSTER_PEERS: &str = "pulse_cluster_peers";
const CLUSTER_SIZE: &str = "pulse_cluster_size";
const CLUSTER_SIZE_MAX: &str = "pulse_cluster_size_max";

pub fn handshake(protocol: &'static str, outcome: &'static str) {
    counter!(HANDSHAKES, "protocol" => protocol, "outcome" => outcome).increment(1);
}

pub fn cluster_lookup_requested() {
    counter!(CLUSTER_LOOKUPS).increment(1);
}

pub fn cluster_lookup_submitted() {
    counter!(CLUSTER_LOOKUP_REPLIES).increment(1);
}

const NATS_PUBLISHED: &str = "pulse_nats_published_total";
const NATS_PUBLISH_FAILED: &str = "pulse_nats_publish_failed_total";
const NATS_DROPPED: &str = "pulse_nats_dropped_total";
const NATS_SUPERSEDED: &str = "pulse_nats_superseded_total";
const NATS_RECONNECTS: &str = "pulse_nats_reconnects_total";
const NATS_CONNECTED: &str = "pulse_nats_connected";

/// Tracks upstream Pulse `ClusterSizeHistogram.BOUNDS`; move it only in lockstep.
/// Exponential over the reachable range -- a cluster cannot exceed the transport's peer ceiling --
/// so resolution is fine where nearly every cluster lands and coarse at the top, where only a
/// collapsed partition reaches.
pub const CLUSTER_SIZE_BUCKETS: [f64; 13] = [
    1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0, 2048.0, 4096.0,
];

/// One sample per cluster per pass, so quantiles are computed at query time.
pub fn cluster_size(size: usize) {
    histogram!(CLUSTER_SIZE).record(size as f64);
}

/// The per-pass totals. `pass_duration_us` is a sum paired with `passes` rather than a
/// pre-averaged gauge, so the mean is a query and stays aggregatable across replicas; `peers` is
/// paired with `count` for the same reason, and `size_max` is kept apart because the histogram's
/// top bucket cannot recover it.
pub fn cluster_pass(
    pass_duration_us: u64,
    reassignments: u64,
    count: usize,
    peers: usize,
    size_max: usize,
) {
    counter!(CLUSTER_PASSES).increment(1);
    counter!(CLUSTER_PASS_DURATION_US).increment(pass_duration_us);
    if reassignments > 0 {
        counter!(CLUSTER_REASSIGNMENTS).increment(reassignments);
    }
    gauge!(CLUSTER_COUNT).set(count as f64);
    gauge!(CLUSTER_PEERS).set(peers as f64);
    gauge!(CLUSTER_SIZE_MAX).set(size_max as f64);
}

/// One per publish whose session differs from the one the wallet's retained assignment was
/// published by -- a second device took the wallet over. A same-session reconnect is not one.
pub fn cluster_takeover() {
    counter!(CLUSTER_TAKEOVERS).increment(1);
}

/// Handed to the client without it throwing. A hand-off rather than a receipt: core NATS never
/// acknowledges a PUB, so a socket dying while the connection still reads as open lands here
/// exactly as a delivered message does.
pub fn nats_published() {
    counter!(NATS_PUBLISHED).increment(1);
}

/// Publishes that failed client-side -- a timeout, a connect failure, an oversized payload. The
/// lever is the broker or the path to it, never the outbox capacity.
pub fn nats_publish_failed() {
    counter!(NATS_PUBLISH_FAILED).increment(1);
}

/// Genuine loss to eviction: more distinct peers held an undelivered assignment at once than the
/// outbox capacity, so the longest-admitted one was pushed out. The actionable capacity signal.
pub fn nats_dropped() {
    counter!(NATS_DROPPED).increment(1);
}

/// Replaced before delivery by a newer message for the same subject. Harmless: the replacement
/// carries strictly fresher state.
pub fn nats_superseded() {
    counter!(NATS_SUPERSEDED).increment(1);
}

pub fn nats_reconnected() {
    counter!(NATS_RECONNECTS).increment(1);
}

/// 1 while the broker connection is up, 0 otherwise. Stays 0 in stats-only mode; set on every exit
/// path of a connection so a rebuild cannot strand it at 1.
pub fn nats_connected(connected: bool) {
    gauge!(NATS_CONNECTED).set(if connected { 1.0 } else { 0.0 });
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_connection_gauge_renders_at_zero_before_any_connection() {
        let rendered = render_recorded(|| nats_connected(false));

        assert!(
            rendered.contains("pulse_nats_connected 0"),
            "the alert must arm from process start: `{rendered}`"
        );
    }

    #[test]
    fn the_cluster_size_histogram_renders_with_buckets_not_as_a_summary() {
        let rendered = render_recorded(|| cluster_size(3));

        assert!(
            rendered.contains("pulse_cluster_size_bucket"),
            "a bucket override left off turns the metric into an unaggregatable summary: `{rendered}`"
        );
    }
}
