# Content sync + snapshots

The sync module lives at `crates/catalyrst-server/src/sync/{sync_orchestrator,deploy_remote_entity,pointer_changes,snapshots,retry_failed,backends}.rs`, run by `catalyrst-live`. Snapshot generation: `crates/catalyrst-db/src/{snapshot_generator,snapshots_repository}.rs`.

## Sync pipeline

Pulls deployments from a pool of upstream catalyst peers (`SYNC_SOURCE`; `SYNC_ENABLED=false` by default), fail-soft: slow/wrong peers are dropped, another tried.

**Phase 6 (`resolve_deleter_deployments`) is skipped on resume - on purpose.** Bootstrap order: peer discovery, snapshot ingest, active-entity hydration, pointer-change catch-up, live forward sync, then this O(n^2) full-table self-join. On resume (`frontier > 0`) the live deployer maintains `deleter_deployment` incrementally; running the ~13 min bulk join inline blocks the `PartiallySynced -> Syncing` flip on every restart. Recompute only as one-shot maintenance.

Download/verification rules:

- Random peer start index per download (`deploy_remote_entity`); retries walk the pool (index 0 collapses traffic onto one peer; round-robin correlates starts).
- Blobs are CID-verified before `storage.store()`: wrong bytes = transient, next peer; removing the check trusts every peer.
- 404 is per-peer, not per-hash: a hash goes to `failed_deployments` (permanent) only after the whole pool 404s within the retry budget.
- `snapshots::parse_snapshot` tolerates a few bad lines (counts `num_parse_errors`, warns): silent drops hide format changes; hard-fail halts on cosmetic garbage.
- `/pointer-changes` `next` URLs are relative, query-only (`?from=&to=&limit=&lastId=`): resolve against the current request URL; `url::Url::join` on the server base drops the path, 404s. Test: `pointer_changes::test_resolve_url_query_only_keeps_path`.
- `retry_failed` re-attempts `failed_deployments` on a slow interval with its own `peer_servers` pool; `SYNC_RETRY_CONCURRENCY` (default 10) rows/pass; rows older than `RETRY_FAILED_PRUNE_TTL_DAYS` (default 7) pruned; `RETRY_FAILED_ENABLED=false` disables.
- Two-tier timeouts: `read_timeout` (inter-byte, resets per chunk), `timeout` (whole-request); a stalled peer frees its slot in ~25 s.
- Snapshot files served from `/contents/<hash>` (main store) - no separate bucket.

## Frontier + heartbeat

Two additive `system_properties` keys (old binaries ignore them; missing = unknown; trait default no-op - non-Postgres `DeploymentRepository` impls unaffected):

- `sync_frontier` (epoch ms) - position: greatest fully-applied pointer-changes timestamp; advances only when deployments land.
- `sync_heartbeat` (epoch ms) - liveness: written per fetched pointer-changes page (max one/10 s), at bootstrap-phase ends. Empty pages beat (quiet network = beating, stale frontier; wedged loop = silent). Per-page, not per-call: the live call long-polls (`wait_time_ms > 0`), never returns at the tip - a completion-time beat reads permanently down on a healthy node.

`GET /content/status` gains `syncFrontier`/`lastHeartbeat`/`up` (heartbeat < 300 s) in `synchronizationStatus` once enabled and beaten; read-only nodes emit none (byte-compatible). Caveats: `synchronizationState` never reads "Synced" (post-bootstrap = `Syncing`); `lastSyncWithDAO` backfills `now()` - use `lastHeartbeat`; admin sync-pause reads `up:false` past the threshold.

`/metrics`: `catalyrst_sync_{heartbeat,frontier}_timestamp_seconds` (set at write time, present after first beat). NixOS-module alerts `SyncHeartbeatStale`, `SyncIngestSilent` (via `catalyrst_sync_deployments_total`): [operations.md](./operations.md). Alert on the heartbeat, never the persisted frontier (advances only at phase ends; legitimately stale). At-the-tip checks: frontier lag, ingest rate, upstream A/B, snapshot-hash equality.

```sql
SELECT key, to_timestamp(value::bigint/1000) FROM system_properties
WHERE key IN ('sync_frontier','sync_heartbeat');
```

## Snapshot generation

Deterministic, content-addressed dumps of active deployments per time window; peers bootstrap from them, so every node must emit the same CID per window or network sync diverges. Byte-level parity with `catalyst/src/logic/time-range.ts`, `src/adapters/snapshots-repository/component.ts`.

Time buckets are NOT calendar units (match `divideTimeInYearsMonthsWeeksAndDays`; no `chrono::Month`/`Duration::days(365)` - CIDs depend on exact day counts):

| Unit | Days | Notes |
|---|---|---|
| day | 1 | |
| week | 7 | |
| month | 28 | exactly 4 weeks |
| year | 336 | 12 such "months" |

Division threshold (`intervalSizes[idx+1] ?? intervalSize` block-fit predicate: one current-size block AND one fully populated block at every finer level):

```
divide if number_of_next_in_current * next_size
          + next_size_1 + next_size_2 + next_size_3  <= window
```

Invariants:

- Boundary double-count is canonical: adjacent intervals share endpoints (`interval[i].end == interval[i+1].init`); an entity on a shared boundary lands in both snapshots. Test `entity_on_shared_boundary_is_counted_in_both_windows_inclusive` guards the inclusive upper bound (exclusive would change the canonical hash).
- `BETWEEN init AND end` is load-bearing at 3 sites (`stream_active_deployments_in_time_range`, `snapshot_is_outdated`, `get_number_of_active_entities_in_time_range`), all inclusive-inclusive, migrated only in lockstep - upstream flags it "must never change" for snapshot immutability and convergency.
- CID = `@dcl/hashing::hashV1` over the uncompressed file; compressing at rest breaks every sync client.
- `generate_snapshots_multi` keeps exactly one row per current division interval; a file is deleted only when no valid interval references its hash (references span intervals - not delete-on-orphan).
- The trailing remainder is never snapshotted (served live via `/pointer-changes`); generation stops at the last whole interval.
- Canary `division_matches_reference_progression_vectors` pins `(days, "YMWI...")` pairs from `time-range.spec.ts`; changing bucket sizes, the formula, or iteration order fails it.

`/content/{snapshots,failed-deployments}` legitimately differ across peers (conformance skips via `volatility.toml`); the only valid cross-node assertion: window-T snapshot bytes hash-equal across any peers synced past T - bytes, not listings.
