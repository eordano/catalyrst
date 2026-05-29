# Snapshot generation and CID convergency

Snapshots are how catalyst peers bootstrap each other. A snapshot is a
deterministic, content-addressed dump of the active deployments inside a
time window. Every catalyst node must produce **the same CID** for the
same window, or peers' snapshot-driven sync diverges and they stop
trusting each other.

This doc collects the invariants that produce CID convergency. They look
arbitrary in isolation; each is reference-implementation parity.

Sources:
- `crates/catalyrst-db/src/snapshot_generator.rs`
- `crates/catalyrst-db/src/snapshots_repository.rs`
- Reference: `catalyst/content/src/logic/time-range.ts`,
  `catalyst/content/src/adapters/snapshots-repository/component.ts`

## Catalyst time-bucket sizes (NOT calendar)

Catalyst's "month" and "year" are fixed sizes, not calendar units:

| Unit   | Days | Notes                          |
|--------|------|--------------------------------|
| day    | 1    |                                |
| week   | 7    |                                |
| month  | 28   | 4 weeks exactly                |
| year   | 336  | 12 of these months             |

These constants live in `snapshot_generator.rs` and must match
`divideTimeInYearsMonthsWeeksAndDays` in the reference's
`time-range.ts`. **Do not** swap in `chrono::Month` or
`chrono::Duration::days(365)` — the snapshot CIDs depend on the exact
day counts above.

## Division threshold formula

The decision whether to keep dividing a window or emit a snapshot at the
current level is:

```
divide if  number_of_next_in_current * next_size  +  next_size_1
                                                 +  next_size_2
                                                 +  next_size_3   <= window
```

This mirrors the reference's `intervalSizes[idx+1] ?? intervalSize`
block-fit predicate: there's enough room to emit one block at the
current size AND keep at least one fully-populated block at every finer
level.

## Contiguous shared-endpoint intervals → boundary entities are double-counted

`divide_time_in_years_months_weeks_and_days` produces intervals that
share endpoints: `interval[i].end == interval[i+1].init`. An entity
whose `entity_timestamp` lands on a shared boundary is intentionally
emitted in **BOTH** adjacent snapshots.

This double-count is canonical and required for CID parity. Test
`entity_on_shared_boundary_is_counted_in_both_windows_inclusive`
guards against any "fix" that makes the upper bound exclusive (which
would break the canonical hash).

## `BETWEEN init AND end` is load-bearing (3 query sites)

All three SQL queries — `stream_active_deployments_in_time_range`,
`snapshot_is_outdated`, and `get_number_of_active_entities_in_time_range`
— use INCLUSIVE-INCLUSIVE `BETWEEN`. The reference query
(`catalyst/content/src/adapters/snapshots-repository/component.ts ::
streamActiveDeploymentsInTimeRange`) carries the explicit upstream
warning:

> "IT IS IMPORTANT THAT THIS QUERY NEVER CHANGES — It ensures the
> snapshots immutability and convergency."

Combined with shared-endpoint intervals (above), boundary entities are
intentionally double-counted across adjacent snapshots. Making the upper
bound exclusive would change snapshot contents and produce CIDs no peer
can reproduce, breaking sync.

**All three sites must move in lockstep** if ever migrated.

## Snapshot hash over UNCOMPRESSED bytes

The snapshot CID is computed via `@dcl/hashing::hashV1` over the
**uncompressed** snapshot file. The file MUST be stored uncompressed so
that the served bytes hash back to the advertised CID. Sync clients
verify against this; compressing the file at rest would silently break
all downstream consumers.

## Stale-row pruning invariant

The `generate_snapshots_multi` post-loop pruning enforces "exactly one
snapshot row per current division interval". It orphans:

- Legacy consolidated full-range snapshots (from earlier catalyst versions).
- Intervals that disappeared as the timeline grew (e.g. a week that got
  absorbed into a newer month).

A stale snapshot file is only deleted from storage if no valid interval
snapshot still references that hash. Avoid simplifying this to
"delete-on-orphan" — file references can persist across multiple
intervals.

## Trailing remainder is NOT snapshotted

The `remainder` window at the end of the time division is intentionally
NOT generated as a snapshot — catalyst serves it live via
`/pointer-changes`. Snapshot generation stops at the last whole
interval.

## Reference progression test vectors

`division_matches_reference_progression_vectors` pins pairs of
`(days, "YMWI..."-encoded representation)` taken verbatim from catalyst's
`time-range.spec.ts ::should satisfy progression` test. Base timestamp
`1_640_995_200_000` (2022-01-01 UTC) is the reference base; the
breakdown is base-independent so any base works equivalently.

If you change time-bucket sizes, the division formula, or the iteration
order, this test will fail — that failure is the canary signaling that
snapshot CIDs will diverge from the rest of the network.

## When snapshots are useful for parity testing

State-dependent endpoints (`/content/snapshots`, `/content/failed-deployments`)
diff legitimately across regions because each catalyst has synced past a
slightly different point. The conformance suite skips both via
`volatility.toml`. Only meaningful parity assertion: a snapshot at time
T from one peer must hash-equal a snapshot at time T from another peer
**that has synced past T**, comparing the snapshot file bytes directly,
not the listing.
