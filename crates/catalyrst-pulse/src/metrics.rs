//! Scene-listener metrics over the `metrics` facade (exporter-agnostic; a no-op recorder when
//! none is installed, so these are safe to call from tests).

use metrics::{counter, gauge, histogram};

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
