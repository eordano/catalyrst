# Bench slow-query audit - 2026-09-17

Reviewed the preceding 24 hours of PostgreSQL logs, Catalyrst service logs, and host warning/error logs. PostgreSQL logs statements over 1 second. `pg_stat_statements` is not loaded, so this is a slow-statement audit, not an exhaustive cost profile of every fast query. Temporary-file counters since server startup are not daily totals.

## Findings

- Trade materialized-view refresh: 2,841 logged calls, approximately 10,192 seconds total. After the earlier work-memory change, average elapsed time remained about 4 seconds. Do not describe that change as solving refresh latency.
- Old parcel-map query: 1,396 logged calls, approximately 2,493 seconds total. The already-deployed estate CTE fix measured 193 ms in this audit; no additional map change was justified.
- Legacy catalog: four approximately 1.6-1.8 second calls. Investigate its non-indexable expiry conversions separately from the normalized v2 path.
- Content startup loads 2.8 million entity IDs for its Bloom filter. This scan occurs at startup, not on every request.
- Historical content overwrite repair reaches its 60-second timeout; its anti-join plan estimates billions of intermediate rows.
- Snapshot staleness checks need only one matching late arrival, but their SQL did not express an early stop.
- Presence queries measured 0.03-0.24 ms with existing indexes. No changes justified.
- Long `COPY ... TO stdout` statements are scheduled backups. Diagnostic EXPLAIN/equivalence queries are excluded from application bottlenecks.
- No OOM, disk-full, I/O-error, or deadlock patterns appeared in host warning/error logs during the audit window.

## Snapshot check

Added `LIMIT 1` to `snapshot_is_outdated`, preserving the inclusive entity time range, strictly newer local timestamp, and active-deployment filter. A representative warm-cache production read-only comparison was 4.264 ms / 638 buffer accesses before versus 0.035 ms / 5 accesses afterward. This is a positive-match example; a no-match case still needs to establish absence.

The isolated PostgreSQL regression test covers both inclusive endpoints, records outside the range, equal generation timestamps, overwritten deployments, and many matches.

## Catalog expiry filter

V1 now uses `expires_at_normalized > NOW()` with the existing index. It retains the original upper bound and numeric equivalents of the legacy 10/13-digit acceptance rules. The measured order aggregation improved from 321.9 ms to 124.9 ms. This is a subquery measurement, not a promise of a 2.6x endpoint speedup.

A production comparison found zero predicate mismatches across 1,165,458 orders, including 8,250 beyond the upper limit. The persisted PostgreSQL boundary test executes the predicate extracted from the actual generated v1 query, so future changes to the query are covered. It includes NULL, negative, expired, seconds, milliseconds, and unsupported timestamp widths.

## Rejected candidates

- Folding duplicate ownership validation into the trade view's main aggregate matched all 29,303 output rows, including JSON assets and status. Source SELECT time changed from 2,740.613 ms to 2,728.575 ms (0.4%). No migration was justified.
- Sharing a materialized NFT subset between the original view's two joins made the full source SELECT slower: 5,019.706 ms. It covered both sent and received ERC721 assets and projected only referenced NFT columns. Rejected without production mutation.
- Replacing historical overwrite repair with a per-pointer window avoided the enormous anti-join plan, but its full production read-only candidate still exceeded a 30-second bound. A safe bounded repair design remains necessary. Numeric row IDs must not replace lexical entity-ID tie-breaking, NULL pointers must not count as overlaps, and duplicate pointers/concurrent updates need explicit handling. No unproven rewrite was deployed.

## Deployment and validation

- Snapshot fix: `/opt/catalyrst-sync/releases/20260917-snapshot-probe`; preserves the existing credential-loading wrapper and four-peer configuration. Runtime override `zzzzz-snapshot-20260917.conf` under `catalyrst-sync.service.d`.
- Catalog fix: `/opt/catalyrst/releases/data-catalog-20260917`; preserves price polling and all service environment. Runtime override `zzzz-catalog-20260917.conf` under `catalyrst-data.service.d`.
- Both scoped deployments used checksum/library checks and automatic readiness rollback. All data members and content sync report healthy; no failed systemd units.
- Content unit suite: 337 passed, 2 ignored. Snapshot PostgreSQL boundary regression and generated catalog predicate compatibility regression passed on isolated PostgreSQL 17. Release builds and diff checks passed.
- Runtime overrides are under `/run/systemd/system` and require normal Nix integration for reboot persistence. Source changes are in the working tree; unrelated existing changes were retained.

Trade refresh remains approximately four seconds every 30 seconds. Its refresh interval, correctness rules, and error visibility were not relaxed to manufacture a performance gain.
