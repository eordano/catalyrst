use std::collections::HashSet;
use std::io::Write;
use std::sync::{LazyLock, Mutex};

use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::snapshots_repository::{self, SnapshotMetadata, TimeRange};

pub const SNAPSHOTS_INIT_TIMESTAMP_MS: f64 = 1_577_836_800_000.0;

const SNAPSHOT_HEADER: &str = "### Decentraland json snapshot";

const MS_PER_DAY: f64 = 86_400_000.0;
const MS_PER_WEEK: f64 = 7.0 * MS_PER_DAY;
const MS_PER_MONTH: f64 = 4.0 * MS_PER_WEEK;
const MS_PER_YEAR: f64 = 12.0 * MS_PER_MONTH;

pub fn divide_time_in_years_months_weeks_and_days(
    time_range: TimeRange,
) -> (Vec<TimeRange>, TimeRange) {
    let time_size_ms = time_range.end_timestamp - time_range.init_timestamp;
    let interval_sizes = [MS_PER_YEAR, MS_PER_MONTH, MS_PER_WEEK, MS_PER_DAY];

    let mut intervals: Vec<TimeRange> = Vec::new();
    let mut remaining = time_size_ms;
    let mut init_interval = time_range.init_timestamp;

    for idx in 0..interval_sizes.len() {
        let interval_size = interval_sizes[idx];
        let next_size = *interval_sizes.get(idx + 1).unwrap_or(&interval_size);
        let number_of_next_in_current = (interval_size / next_size).floor();

        let next1 = *interval_sizes.get(idx + 1).unwrap_or(&0.0);
        let next2 = *interval_sizes.get(idx + 2).unwrap_or(&0.0);
        let next3 = *interval_sizes.get(idx + 3).unwrap_or(&0.0);

        let threshold = number_of_next_in_current * next_size + next1 + next2 + next3;

        while remaining >= threshold {
            let end_interval = init_interval + interval_size;
            intervals.push(TimeRange::new(init_interval, end_interval));
            init_interval = end_interval;
            remaining -= interval_size;
        }
    }

    let remainder = TimeRange::new(init_interval, time_range.end_timestamp);
    (intervals, remainder)
}

/// Hashes whose stored bytes have been re-read and matched during this process's life.
///
/// Content-addressed storage is immutable, so a match is permanent -- the file under a key cannot
/// change, so the verdict cannot expire. Re-deriving it every cycle would mean re-reading every
/// reused snapshot, gigabytes an hour, to learn the same fact.
static VERIFIED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

fn remember_verified(hash: &str) {
    if let Ok(mut seen) = VERIFIED.lock() {
        seen.insert(hash.to_owned());
    }
}

fn already_verified(hash: &str) -> bool {
    VERIFIED
        .lock()
        .map(|seen| seen.contains(hash))
        .unwrap_or(false)
}

/// The metadata to reuse for `candidate`, or `None` when the interval has to be regenerated.
///
/// Reuse is the one path that republishes a hash without recomputing it, so it is the one path that
/// has to check. `exist()` answers "is there a file at this key", which is not the same question: a
/// bulk import in 2026-05 wrote five snapshots whose CIDs it had computed with a broken multi-level
/// DAG, and because those intervals are frozen history the reuse gate re-advertised them every cycle
/// for months. Every peer that fetched one got bytes that did not hash to the CID we had named, and
/// rejected the payload.
///
/// When bytes and key disagree the bytes are the truth -- the snapshot is good and only its name is
/// wrong, so this re-keys rather than throwing away valid content.
async fn reusable_snapshot(
    pool: &PgPool,
    content_storage: &catalyrst_storage::ContentStorage,
    candidate: &SnapshotMetadata,
) -> Option<SnapshotMetadata> {
    let advertised = candidate.hash.as_deref()?;

    if already_verified(advertised) {
        return Some(candidate.clone());
    }

    let actual = match content_storage.stored_content_hash(advertised).await {
        Ok(Some(actual)) => actual,
        Ok(None) => return None,
        Err(e) => {
            error!(hash = advertised, %e, "Failed to read snapshot content to verify its hash");
            return None;
        }
    };

    if actual == advertised {
        remember_verified(advertised);
        return Some(candidate.clone());
    }

    warn!(
        advertised,
        actual, "Stored snapshot does not hash to its advertised CID; re-keying"
    );

    // Renamed before the row moves: a crash in between leaves the row pointing at a key that is now
    // absent, which regenerates on the next cycle. The other order would leave the wrong CID still
    // serving its mismatched bytes, which is the defect itself.
    match content_storage.rekey(advertised, &actual).await {
        Ok(true) => {}
        Ok(false) => return None,
        Err(e) => {
            error!(advertised, actual, %e, "Failed to re-key snapshot content");
            return None;
        }
    }

    match snapshots_repository::update_snapshot_hash(
        pool,
        candidate.time_range,
        advertised,
        &actual,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            warn!(
                advertised,
                "Snapshot row already moved by another writer; regenerating"
            );
            return None;
        }
        Err(e) => {
            error!(advertised, actual, %e, "Failed to re-key snapshot row");
            return None;
        }
    }

    remember_verified(&actual);

    Some(SnapshotMetadata {
        hash: Some(actual),
        ..candidate.clone()
    })
}

pub async fn generate_snapshot(
    pool: &PgPool,
    content_storage: &catalyrst_storage::ContentStorage,
    init_timestamp_ms: f64,
    end_timestamp_ms: f64,
    replaced_hashes: Vec<String>,
) -> Result<SnapshotMetadata, Box<dyn std::error::Error + Send + Sync>> {
    let time_range = TimeRange::new(init_timestamp_ms, end_timestamp_ms);

    let generation_timestamp = chrono::Utc::now().timestamp_millis() as f64;

    let deployments =
        snapshots_repository::stream_active_deployments_in_time_range(pool, time_range).await?;

    let number_of_entities = i32::try_from(deployments.len()).map_err(
        |_| -> Box<dyn std::error::Error + Send + Sync> {
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "snapshot deployment count {} overflows i32 (schema column type)",
                deployments.len()
            ))
        },
    )?;

    let mut buf: Vec<u8> = Vec::new();
    writeln!(buf, "{}", SNAPSHOT_HEADER)?;

    for dep in &deployments {
        let line = serde_json::to_string(dep)?;
        writeln!(buf, "{}", line)?;
    }

    let hash = catalyrst_hashing::hash_bytes_v1(&buf);

    let content_bytes: bytes::Bytes = buf.into();
    content_storage
        .store(&hash, content_bytes)
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    let replaced_snapshot_hashes = replaced_hashes
        .into_iter()
        .filter(|h| h != &hash)
        .collect::<Vec<_>>();

    let metadata = SnapshotMetadata {
        hash: Some(hash),
        time_range,
        replaced_snapshot_hashes,
        number_of_entities,
        generation_timestamp,
    };

    snapshots_repository::save_snapshot(pool, &metadata).await?;

    info!(
        hash = metadata.hash.as_deref().unwrap_or(""),
        entities = number_of_entities,
        init = init_timestamp_ms as i64,
        end = end_timestamp_ms as i64,
        "Snapshot generated"
    );

    Ok(metadata)
}

pub async fn generate_snapshots_multi(
    pool: &PgPool,
    content_storage: &catalyrst_storage::ContentStorage,
    init_timestamp_ms: f64,
    end_timestamp_ms: f64,
) -> Result<Vec<SnapshotMetadata>, Box<dyn std::error::Error + Send + Sync>> {
    let whole = TimeRange::new(init_timestamp_ms, end_timestamp_ms);
    let (intervals, remainder) = divide_time_in_years_months_weeks_and_days(whole);

    info!(
        intervals = intervals.len(),
        remainder_init = remainder.init_timestamp as i64,
        remainder_end = remainder.end_timestamp as i64,
        "Dividing snapshot time range"
    );

    let mut result: Vec<SnapshotMetadata> = Vec::with_capacity(intervals.len());

    for interval in intervals {
        let saved =
            snapshots_repository::find_snapshots_strictly_contained_in_time_range(pool, interval)
                .await?;

        let exact: Vec<&SnapshotMetadata> = saved
            .iter()
            .filter(|s| {
                (s.time_range.init_timestamp - interval.init_timestamp).abs() < 1.0
                    && (s.time_range.end_timestamp - interval.end_timestamp).abs() < 1.0
            })
            .collect();

        let mut reused: Option<SnapshotMetadata> = None;
        if exact.len() == 1 {
            let candidate = exact[0];
            let outdated = snapshots_repository::snapshot_is_outdated(pool, candidate)
                .await
                .unwrap_or(true);
            if !outdated {
                reused = reusable_snapshot(pool, content_storage, candidate).await;
            }
        }

        if let Some(meta) = reused {
            result.push(meta);
            continue;
        }

        let stale_hashes: Vec<String> = saved.iter().filter_map(|s| s.hash.clone()).collect();
        if !stale_hashes.is_empty() {
            let keep = snapshots_repository::get_snapshot_hashes_not_in_time_range(
                pool,
                &stale_hashes,
                interval,
            )
            .await
            .unwrap_or_default();
            if snapshots_repository::delete_snapshots_in_time_range(pool, &stale_hashes, interval)
                .await
                .is_ok()
            {
                for h in &stale_hashes {
                    if !keep.contains(h) {
                        content_storage.delete(h).await.ok();
                    }
                }
            }
        }

        let meta = generate_snapshot(
            pool,
            content_storage,
            interval.init_timestamp,
            interval.end_timestamp,
            stale_hashes,
        )
        .await?;
        result.push(meta);
    }

    let valid_hashes: std::collections::HashSet<String> =
        result.iter().filter_map(|m| m.hash.clone()).collect();
    let valid_ranges: Vec<TimeRange> = result.iter().map(|m| m.time_range).collect();

    match snapshots_repository::get_all_snapshots(pool).await {
        Ok(all) => {
            for snap in all {
                let matches_interval = valid_ranges.iter().any(|iv| {
                    (iv.init_timestamp - snap.time_range.init_timestamp).abs() < 1.0
                        && (iv.end_timestamp - snap.time_range.end_timestamp).abs() < 1.0
                });
                if matches_interval {
                    continue;
                }
                if snapshots_repository::delete_snapshot_by_time_range(pool, snap.time_range)
                    .await
                    .is_ok()
                {
                    if let Some(h) = &snap.hash {
                        if !valid_hashes.contains(h) {
                            content_storage.delete(h).await.ok();
                        }
                    }
                }
                info!(
                    init = snap.time_range.init_timestamp as i64,
                    end = snap.time_range.end_timestamp as i64,
                    "Pruned stale snapshot row outside division intervals"
                );
            }
        }
        Err(e) => {
            error!(%e, "Failed to load snapshots for stale-row pruning");
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_matches_reference_shape() {
        let init = SNAPSHOTS_INIT_TIMESTAMP_MS;
        let end = init + (6.0 * 365.0 + 142.0) * MS_PER_DAY;
        let (intervals, remainder) =
            divide_time_in_years_months_weeks_and_days(TimeRange::new(init, end));

        assert!(!intervals.is_empty());
        let mut prev_end = init;
        for iv in &intervals {
            assert!(
                (iv.init_timestamp - prev_end).abs() < 1.0,
                "intervals must be contiguous"
            );
            assert!(iv.end_timestamp > iv.init_timestamp);
            prev_end = iv.end_timestamp;
        }
        assert!((remainder.init_timestamp - prev_end).abs() < 1.0);
        assert!(remainder.end_timestamp >= remainder.init_timestamp);

        let first_span = intervals[0].end_timestamp - intervals[0].init_timestamp;
        assert!(
            (first_span - MS_PER_YEAR).abs() < 1.0,
            "first interval should be a year"
        );
    }

    #[test]
    fn division_empty_when_too_small() {
        let init = SNAPSHOTS_INIT_TIMESTAMP_MS;
        let end = init + MS_PER_DAY / 2.0;
        let (intervals, remainder) =
            divide_time_in_years_months_weeks_and_days(TimeRange::new(init, end));
        assert!(intervals.is_empty());
        assert!((remainder.init_timestamp - init).abs() < 1.0);
        assert!((remainder.end_timestamp - end).abs() < 1.0);
    }

    fn division_representation(start_ms: f64, number_of_days: f64) -> String {
        let init = start_ms;
        let end = init + MS_PER_DAY * number_of_days;
        let (intervals, _remainder) =
            divide_time_in_years_months_weeks_and_days(TimeRange::new(init, end));
        let mut repr = String::new();
        for iv in &intervals {
            let size = iv.end_timestamp - iv.init_timestamp;
            repr.push(if (size - MS_PER_DAY).abs() < 1.0 {
                'I'
            } else if (size - MS_PER_WEEK).abs() < 1.0 {
                'W'
            } else if (size - MS_PER_MONTH).abs() < 1.0 {
                'M'
            } else if (size - MS_PER_YEAR).abs() < 1.0 {
                'Y'
            } else {
                '-'
            });
        }
        repr
    }

    #[test]
    fn division_matches_reference_progression_vectors() {
        const BASE: f64 = 1_640_995_200_000.0;

        let cases: &[(f64, &str)] = &[
            (1.0, "I"),
            (7.0, "IIIIIII"),
            (8.0, "WI"),
            (14.0, "WIIIIIII"),
            (15.0, "WWI"),
            (21.0, "WWIIIIIII"),
            (22.0, "WWWI"),
            (28.0, "WWWIIIIIII"),
            (29.0, "WWWWI"),
            (34.0, "WWWWIIIIII"),
            (35.0, "WWWWIIIIIII"),
            (36.0, "MWI"),
            (42.0, "MWIIIIIII"),
            (43.0, "MWWI"),
            (49.0, "MWWIIIIIII"),
            (50.0, "MWWWI"),
            (56.0, "MWWWIIIIIII"),
            (57.0, "MWWWWI"),
            (63.0, "MWWWWIIIIIII"),
            (64.0, "MMWI"),
            (364.0, "MMMMMMMMMMMMWWWIIIIIII"),
            (365.0, "MMMMMMMMMMMMWWWWI"),
            (370.0, "MMMMMMMMMMMMWWWWIIIIII"),
            (371.0, "MMMMMMMMMMMMWWWWIIIIIII"),
            (372.0, "YMWI"),
        ];

        for (days, expected) in cases {
            assert_eq!(
                division_representation(BASE, *days),
                *expected,
                "division of {days} days did not match reference progression vector"
            );
        }
    }

    #[test]
    fn division_genesis_to_fixed_end_exact_counts() {
        let init = SNAPSHOTS_INIT_TIMESTAMP_MS;
        let end = init + 372.0 * MS_PER_DAY;
        let (intervals, remainder) =
            divide_time_in_years_months_weeks_and_days(TimeRange::new(init, end));

        let count_of = |size: f64| {
            intervals
                .iter()
                .filter(|iv| ((iv.end_timestamp - iv.init_timestamp) - size).abs() < 1.0)
                .count()
        };

        assert_eq!(count_of(MS_PER_YEAR), 1, "expected exactly 1 year interval");
        assert_eq!(
            count_of(MS_PER_MONTH),
            1,
            "expected exactly 1 month interval"
        );
        assert_eq!(count_of(MS_PER_WEEK), 1, "expected exactly 1 week interval");
        assert_eq!(count_of(MS_PER_DAY), 1, "expected exactly 1 day interval");
        assert_eq!(intervals.len(), 4, "expected exactly 4 intervals total");

        let remainder_ms = remainder.end_timestamp - remainder.init_timestamp;
        assert!(
            remainder_ms.abs() < 1.0,
            "expected zero remainder, got {remainder_ms} ms"
        );
        assert!((remainder.init_timestamp - end).abs() < 1.0);
    }

    fn in_window(ts: f64, w: TimeRange) -> bool {
        ts >= w.init_timestamp && ts <= w.end_timestamp
    }

    #[test]
    fn entity_on_shared_boundary_is_counted_in_both_windows_inclusive() {
        let init = SNAPSHOTS_INIT_TIMESTAMP_MS;
        let end = init + 8.0 * MS_PER_DAY;
        let (intervals, _remainder) =
            divide_time_in_years_months_weeks_and_days(TimeRange::new(init, end));
        assert_eq!(intervals.len(), 2, "8 days should divide into week + day");

        let shared_boundary = intervals[0].end_timestamp;
        assert!(
            (shared_boundary - intervals[1].init_timestamp).abs() < 1.0,
            "intervals must share the boundary"
        );

        assert!(
            in_window(shared_boundary, intervals[0]),
            "boundary entity must be in the first (week) window"
        );
        assert!(
            in_window(shared_boundary, intervals[1]),
            "boundary entity must be in the second (day) window"
        );

        let windows_containing = intervals
            .iter()
            .filter(|iv| in_window(shared_boundary, **iv))
            .count();
        assert_eq!(
            windows_containing, 2,
            "with the canonical inclusive BETWEEN, a shared-boundary entity is \
             counted in exactly 2 adjacent windows (double-count is intentional \
             for CID parity)"
        );

        let inside_first = intervals[0].init_timestamp + MS_PER_DAY;
        let only_first = intervals
            .iter()
            .filter(|iv| in_window(inside_first, **iv))
            .count();
        assert_eq!(
            only_first, 1,
            "a non-boundary entity is in exactly one window"
        );
    }
}
