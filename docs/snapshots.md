# Snapshot generation and CID convergency

> Status: distilled 2026-07-04; invariants last re-verified against code
> 2026-07-03 (docs-stale-audit).

Snapshots are how catalyst peers bootstrap each other: a deterministic,
content-addressed dump of the active deployments inside a time window. Every
node must produce **the same CID for the same window** or snapshot-driven sync
diverges network-wide. The rules below look arbitrary in isolation; each one is
byte-level parity with the reference implementation
(`catalyst/src/logic/time-range.ts`,
`catalyst/src/adapters/snapshots-repository/component.ts`).

Sources: `crates/catalyrst-db/src/{snapshot_generator,snapshots_repository}.rs`.

## Catalyst time buckets are NOT calendar units

| Unit  | Days | Notes            |
|-------|------|------------------|
| day   | 1    |                  |
| week  | 7    |                  |
| month | 28   | exactly 4 weeks  |
| year  | 336  | 12 such "months" |

These constants must match `divideTimeInYearsMonthsWeeksAndDays` in the
reference. Do not swap in `chrono::Month` or `Duration::days(365)` — snapshot
CIDs depend on the exact day counts.

## Division threshold formula

Keep dividing a window vs emit a snapshot at the current level:

```
divide if number_of_next_in_current * next_size
          + next_size_1 + next_size_2 + next_size_3  <= window
```

This mirrors the reference's `intervalSizes[idx+1] ?? intervalSize` block-fit
predicate: room for one block at the current size AND at least one fully
populated block at every finer level.

## Boundary entities are double-counted — canonically

Adjacent intervals share endpoints (`interval[i].end == interval[i+1].init`).
An entity whose `entity_timestamp` lands on a shared boundary is intentionally
emitted in **both** snapshots. Test
`entity_on_shared_boundary_is_counted_in_both_windows_inclusive` guards against
any "fix" that makes the upper bound exclusive — that would change the
canonical hash.

## `BETWEEN init AND end` is load-bearing at 3 query sites

`stream_active_deployments_in_time_range`, `snapshot_is_outdated`, and
`get_number_of_active_entities_in_time_range` all use inclusive-inclusive
`BETWEEN`. The reference query carries the upstream warning:

> "IT IS IMPORTANT THAT THIS QUERY NEVER CHANGES — It ensures the snapshots
> immutability and convergency."

All three sites must move in lockstep if ever migrated.

## Snapshot hash is over UNCOMPRESSED bytes

The CID is `@dcl/hashing::hashV1` over the uncompressed file, and the file is
stored uncompressed so the served bytes hash back to the advertised CID.
Compressing at rest would silently break every sync client.

## Stale-row pruning

`generate_snapshots_multi` enforces exactly one snapshot row per current
division interval, orphaning legacy consolidated snapshots and intervals
absorbed by timeline growth. A snapshot **file** is deleted only when no valid
interval still references its hash — don't simplify to delete-on-orphan; file
references can span intervals.

## The trailing remainder is not snapshotted

The remainder window at the end of the division is served live via
`/pointer-changes`; snapshot generation stops at the last whole interval.

## Reference progression vectors are the canary

`division_matches_reference_progression_vectors` pins `(days, "YMWI…")` pairs
verbatim from catalyst's `time-range.spec.ts`. If you change bucket sizes, the
division formula, or iteration order, this test fails — that failure means
your snapshot CIDs will diverge from the rest of the network.

## Parity-testing note

`/content/snapshots` and `/content/failed-deployments` legitimately differ
across peers (each has synced past a different point); the conformance suite
skips both via `volatility.toml`. The only meaningful cross-node assertion:
a snapshot for window T from one peer must hash-equal the same window from any
peer that has synced past T — comparing file bytes, not listings.
