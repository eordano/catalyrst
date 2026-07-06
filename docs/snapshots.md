# Snapshot generation and CID convergency

Snapshots bootstrap catalyst peers: a deterministic, content-addressed dump
of the active deployments inside a time window. Every node must produce the
same CID for the same window or snapshot-driven sync diverges network-wide.
Each rule below is byte-level parity with the reference
(`catalyst/src/logic/time-range.ts`,
`catalyst/src/adapters/snapshots-repository/component.ts`). Sources:
`crates/catalyrst-db/src/{snapshot_generator,snapshots_repository}.rs`.

## Catalyst time buckets are NOT calendar units

| Unit  | Days | Notes            |
|-------|------|------------------|
| day   | 1    |                  |
| week  | 7    |                  |
| month | 28   | exactly 4 weeks  |
| year  | 336  | 12 such "months" |

Must match `divideTimeInYearsMonthsWeeksAndDays` in the reference; no
`chrono::Month` or `Duration::days(365)` - snapshot CIDs depend on the exact
day counts.

## Division threshold formula

```
divide if number_of_next_in_current * next_size
          + next_size_1 + next_size_2 + next_size_3  <= window
```

Mirrors the reference's `intervalSizes[idx+1] ?? intervalSize` block-fit
predicate: room for one block at the current size AND at least one fully
populated block at every finer level.

## Other invariants

- Boundary entities are double-counted, canonically: adjacent intervals
  share endpoints (`interval[i].end == interval[i+1].init`); an entity whose
  `entity_timestamp` lands on a shared boundary is emitted in both
  snapshots. Test
  `entity_on_shared_boundary_is_counted_in_both_windows_inclusive` guards
  against an exclusive upper bound - that would change the canonical hash.
- `BETWEEN init AND end` is load-bearing at 3 query sites:
  `stream_active_deployments_in_time_range`, `snapshot_is_outdated`, and
  `get_number_of_active_entities_in_time_range`, all inclusive-inclusive.
  Upstream warning: "IT IS IMPORTANT THAT THIS QUERY NEVER CHANGES - It
  ensures the snapshots immutability and convergency." All three sites move
  in lockstep if ever migrated.
- Snapshot hash is over UNCOMPRESSED bytes: CID = `@dcl/hashing::hashV1`
  over the uncompressed file, stored uncompressed so served bytes hash back
  to the advertised CID. Compressing at rest breaks every sync client.
- Stale-row pruning: `generate_snapshots_multi` enforces exactly one
  snapshot row per current division interval. A snapshot file is deleted
  only when no valid interval still references its hash - not
  delete-on-orphan; file references can span intervals.
- The trailing remainder is not snapshotted: served live via
  `/pointer-changes`; generation stops at the last whole interval.
- Canary: `division_matches_reference_progression_vectors` pins
  `(days, "YMWI...")` pairs verbatim from catalyst's `time-range.spec.ts`.
  Changing bucket sizes, the division formula, or iteration order fails it -
  and means your snapshot CIDs will diverge from the network.

## Parity-testing note

`/content/snapshots` and `/content/failed-deployments` legitimately differ
across peers; the conformance suite skips both via `volatility.toml`. The
only meaningful cross-node assertion: a snapshot for window T from one peer
must hash-equal the same window from any peer synced past T - comparing file
bytes, not listings.
