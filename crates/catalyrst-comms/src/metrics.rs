//! Prometheus series for the Pulse cluster feed, over the `metrics` facade (exporter-agnostic:
//! a no-op recorder when none is installed, so these are safe to call from tests).
//!
//! Series carry the `comms_` prefix, not upstream's `dcl_gatekeeper_`, matching this workspace's
//! per-crate prefix convention (`pulse_`, `catalyrst_http_`); keep every new one on it so a single
//! exposition never needs two prefixes on one dashboard.
//!
//! The exposition rides this service's own HTTP listener at `/metrics` rather than a second bind,
//! so there is no extra port to allocate and no way for the scrape target to be silently absent
//! while the service is up.

use std::sync::OnceLock;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use metrics::{counter, gauge};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

const NATS_CONNECTED: &str = "comms_nats_connected";
const EVENTS_RECEIVED: &str = "comms_cluster_events_received_total";
const CONNECTS_RECEIVED: &str = "comms_cluster_connects_received_total";
const TOKENS_MINTED: &str = "comms_cluster_tokens_minted_total";
const PUBLISHED: &str = "comms_cluster_published_total";
const PUBLISH_FAILED: &str = "comms_cluster_publish_failed_total";
const BANNED_SKIPPED: &str = "comms_cluster_banned_skipped_total";
const ACCESS_CHECK_FAILED: &str = "comms_cluster_access_check_failed_total";
const TAKEOVER_EVICTED: &str = "comms_cluster_takeover_evicted_total";
const TAKEOVER_ABSENT: &str = "comms_cluster_takeover_absent_total";
const TAKEOVER_FAILED: &str = "comms_cluster_takeover_failed_total";
const REANNOUNCE_ATTEMPTED: &str = "comms_cluster_reannounce_attempted_total";
const REANNOUNCE_SUPPRESSED: &str = "comms_cluster_reannounce_suppressed_total";
const REANNOUNCE_UNRESOLVED: &str = "comms_cluster_reannounce_unresolved_total";
const REANNOUNCE_CHECK_FAILED: &str = "comms_cluster_reannounce_check_failed_total";
const REANNOUNCE_SKIPPED_OTHER_SESSION: &str =
    "comms_cluster_reannounce_skipped_other_session_total";

const COUNTERS: [&str; 14] = [
    EVENTS_RECEIVED,
    CONNECTS_RECEIVED,
    TOKENS_MINTED,
    PUBLISHED,
    PUBLISH_FAILED,
    BANNED_SKIPPED,
    ACCESS_CHECK_FAILED,
    TAKEOVER_EVICTED,
    TAKEOVER_ABSENT,
    TAKEOVER_FAILED,
    REANNOUNCE_ATTEMPTED,
    REANNOUNCE_SUPPRESSED,
    REANNOUNCE_UNRESOLVED,
    REANNOUNCE_CHECK_FAILED,
];

/// Installs the recorder `/metrics` renders from.
///
/// A failure here is fatal rather than a warning: the facade would otherwise record into a no-op
/// and `/metrics` would answer 503 with the service itself healthy, which pins `up{job="comms"}`
/// at 0 and burns the shared `ServiceDown` alert for every other job in its regex.
pub fn install_recorder() -> anyhow::Result<()> {
    if HANDLE.get().is_some() {
        return Ok(());
    }
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("failed to install the prometheus recorder: {e}"))?;
    let _ = HANDLE.set(handle);
    register_series();
    Ok(())
}

/// Renders every cluster series at 0 so a dashboard and an alert both have a value to compare
/// against from process start, instead of a panel that stays empty until the first event.
pub fn register_series() {
    nats_connected(false);
    for name in COUNTERS {
        counter!(name).increment(0);
    }
    counter!(REANNOUNCE_SKIPPED_OTHER_SESSION).increment(0);
}

pub async fn metrics_handler() -> Response {
    match HANDLE.get() {
        Some(handle) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            handle.render(),
        )
            .into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "metrics not initialized").into_response(),
    }
}

pub fn nats_connected(up: bool) {
    gauge!(NATS_CONNECTED).set(if up { 1.0 } else { 0.0 });
}

pub fn cluster_event_received() {
    counter!(EVENTS_RECEIVED).increment(1);
}

pub fn cluster_connect_received() {
    counter!(CONNECTS_RECEIVED).increment(1);
}

pub fn cluster_token_minted() {
    counter!(TOKENS_MINTED).increment(1);
}

pub fn cluster_published() {
    counter!(PUBLISHED).increment(1);
}

/// One counter for both failure shapes upstream distinguishes only in its log line: a publish the
/// broker refused, and a publish with no connection to hand the write to.
pub fn cluster_publish_failed() {
    counter!(PUBLISH_FAILED).increment(1);
}

pub fn cluster_banned_skipped() {
    counter!(BANNED_SKIPPED).increment(1);
}

pub fn cluster_access_check_failed() {
    counter!(ACCESS_CHECK_FAILED).increment(1);
}

pub fn cluster_takeover_evicted() {
    counter!(TAKEOVER_EVICTED).increment(1);
}

pub fn cluster_takeover_absent() {
    counter!(TAKEOVER_ABSENT).increment(1);
}

pub fn cluster_takeover_failed() {
    counter!(TAKEOVER_FAILED).increment(1);
}

pub fn cluster_reannounce_attempted() {
    counter!(REANNOUNCE_ATTEMPTED).increment(1);
}

pub fn cluster_reannounce_suppressed() {
    counter!(REANNOUNCE_SUPPRESSED).increment(1);
}

pub fn cluster_reannounce_unresolved() {
    counter!(REANNOUNCE_UNRESOLVED).increment(1);
}

pub fn cluster_reannounce_check_failed() {
    counter!(REANNOUNCE_CHECK_FAILED).increment(1);
}

pub fn cluster_reannounce_skipped_other_session() {
    counter!(REANNOUNCE_SKIPPED_OTHER_SESSION).increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders what `f` recorded through a recorder local to this thread, so a series can be
    /// asserted without installing a global one.
    pub(crate) fn render_recorded<T>(f: impl FnOnce() -> T) -> String {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, f);
        handle.render()
    }

    #[test]
    fn every_cluster_series_renders_before_the_first_event() {
        let rendered = render_recorded(register_series);
        for name in COUNTERS
            .iter()
            .chain([REANNOUNCE_SKIPPED_OTHER_SESSION, NATS_CONNECTED].iter())
        {
            assert!(
                rendered.contains(name),
                "{name} must render at 0 from process start, got:\n{rendered}"
            );
        }
    }

    #[test]
    fn the_connected_gauge_follows_the_link() {
        let rendered = render_recorded(|| {
            nats_connected(false);
            nats_connected(true);
        });
        assert!(rendered.contains("comms_nats_connected 1"));
    }
}
