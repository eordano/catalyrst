use super::*;
use crate::metrics::{RESYNC_OUTCOME_DELTA, RESYNC_OUTCOME_FULL, RESYNC_SEQ_GAP_BUCKETS};

const LATEST_SEQ: u32 = 3;

fn render_after_resync(resync_with_delta: bool, known_seq: u32) -> String {
    let mut w = World::new();
    w.connect(0, "0xobserver");
    w.connect(1, "0xsubject");
    w.teleport(0, 0, v3(8.0, 8.0), "realm-a");
    w.teleport(1, 0, v3(9.0, 8.0), "realm-a");

    let mut sim = PeerSimulation::new(&[50, 100, 200], resync_with_delta);
    let _ = tick(&mut sim, &mut w, 1);

    for i in 0..LATEST_SEQ {
        w.input(1, 0, v3(10.0 + i as f32, 8.0));
    }
    assert_eq!(
        w.board.last_seq(1),
        LATEST_SEQ,
        "a teleport plus LATEST_SEQ inputs leaves the subject at LATEST_SEQ"
    );
    w.peers.get_mut(&0).unwrap().request_resync(1, known_seq);

    let recorder = crate::metrics::prometheus_builder()
        .expect("bucket override accepted")
        .build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        let _ = tick(&mut sim, &mut w, 2);
    });
    handle.render()
}

fn series(render: &str, suffix: &str, outcome: &str, extra: Option<&str>) -> Option<f64> {
    let head = format!("pulse_resync_seq_gap{suffix}{{outcome=\"{outcome}\"");
    render
        .lines()
        .filter(|l| l.starts_with(head.as_str()))
        .find(|l| extra.map(|e| l.contains(e)).unwrap_or(true))
        .map(|l| {
            l.rsplit(' ')
                .next()
                .expect("a rendered line ends in its value")
                .parse()
                .expect("a rendered value parses")
        })
}

fn count(render: &str, outcome: &str) -> f64 {
    series(render, "_count", outcome, None).unwrap_or(0.0)
}

fn sum(render: &str, outcome: &str) -> f64 {
    series(render, "_sum", outcome, None)
        .unwrap_or_else(|| panic!("no {outcome} sum in:\n{render}"))
}

fn bucket(render: &str, outcome: &str, le: &str) -> f64 {
    let extra = format!("le=\"{le}\"");
    series(render, "_bucket", outcome, Some(extra.as_str()))
        .unwrap_or_else(|| panic!("no {outcome} bucket le={le} in:\n{render}"))
}

#[test]
fn resync_full_state_records_the_baseline_gap() {
    let render = render_after_resync(false, 1);

    assert_eq!(count(&render, RESYNC_OUTCOME_FULL), 1.0);
    assert_eq!(sum(&render, RESYNC_OUTCOME_FULL), 2.0, "3 - 1");
    assert_eq!(
        count(&render, RESYNC_OUTCOME_DELTA),
        0.0,
        "a fallback must not count as a targeted delta"
    );
    assert_eq!(
        bucket(&render, RESYNC_OUTCOME_FULL, "1"),
        0.0,
        "a gap of 2 is above the 1 bound"
    );
    assert_eq!(
        bucket(&render, RESYNC_OUTCOME_FULL, "2"),
        1.0,
        "a gap of 2 lands in the 2 bucket, so the buckets are applied and this is not a summary"
    );
}

#[test]
fn resync_targeted_delta_records_the_baseline_gap_under_its_own_outcome() {
    let render = render_after_resync(true, 1);

    assert_eq!(count(&render, RESYNC_OUTCOME_DELTA), 1.0);
    assert_eq!(sum(&render, RESYNC_OUTCOME_DELTA), 2.0, "3 - 1");
    assert_eq!(
        count(&render, RESYNC_OUTCOME_FULL),
        0.0,
        "a request served by targeted delta must not count as a fallback"
    );
}

#[test]
fn resync_gap_is_zero_when_the_baseline_is_already_current() {
    let render = render_after_resync(false, LATEST_SEQ);

    assert_eq!(count(&render, RESYNC_OUTCOME_FULL), 1.0);
    assert_eq!(sum(&render, RESYNC_OUTCOME_FULL), 0.0);
    assert_eq!(
        bucket(&render, RESYNC_OUTCOME_FULL, "0"),
        1.0,
        "a current baseline gets its own bucket, not the one-publish-behind bucket"
    );
}

#[test]
fn resync_gap_clamps_a_baseline_ahead_of_the_latest_seq() {
    for known_seq in [9u32, 2_147_483_652, 3_000_000_000, u32::MAX] {
        let render = render_after_resync(false, known_seq);
        assert_eq!(count(&render, RESYNC_OUTCOME_FULL), 1.0);
        assert_eq!(
            sum(&render, RESYNC_OUTCOME_FULL),
            0.0,
            "known_seq {known_seq} is ahead of the latest publish: it records 0, never a wrapped gap"
        );
    }
}

#[test]
fn both_outcome_series_render_before_the_first_resync() {
    let recorder = crate::metrics::prometheus_builder()
        .expect("bucket override accepted")
        .build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, crate::metrics::register_resync_seq_gap_series);
    let render = handle.render();

    assert_eq!(
        series(&render, "_count", RESYNC_OUTCOME_DELTA, None),
        Some(0.0),
        "an absent series makes a share-of-total query read no-data instead of 0:\n{render}"
    );
    assert_eq!(
        series(&render, "_count", RESYNC_OUTCOME_FULL, None),
        Some(0.0),
        "the fallback outcome is the one a healthy server never samples:\n{render}"
    );
}

#[test]
fn resync_seq_gap_buckets_track_upstream() {
    assert_eq!(
        RESYNC_SEQ_GAP_BUCKETS,
        [0.0, 1.0, 2.0, 4.0, 8.0, 10.0, 16.0, 20.0, 24.0, 32.0, 64.0, 128.0, 256.0, 1024.0],
        "upstream PulseMetrics.Simulation.RESYNC_SEQ_GAP_BUCKETS"
    );
}
