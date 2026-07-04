# Sync pipeline — load-bearing invariants

> Status: distilled 2026-07-04 from the sync code paths; invariants last
> re-verified against code 2026-07-03 (docs-stale-audit).

catalyrst pulls deployments from a pool of upstream catalyst peers
(`SYNC_SOURCE`). Sync is fail-soft: a slow or wrong peer is dropped and another
is tried; bad bytes never poison local storage. Everything below is a rule the
code enforces for a non-obvious reason — each has already been broken once, or
guards a network-wide compatibility property.

Sources: `crates/catalyrst-sync/src/{sync_orchestrator,deploy_remote_entity,pointer_changes,snapshots,peer_cluster,retry_failed}.rs`.

## Phase 6 (resolve `deleter_deployment`) is skipped on resume — on purpose

`SyncOrchestrator` runs six phases on a fresh bootstrap: peer discovery,
snapshot ingest, active-entity hydration, pointer-change catch-up, live forward
sync, and finally `resolve_deleter_deployments` — an O(n²) full-table
self-join. Phase 6 is **deliberately skipped** when resuming
(`frontier > 0`): the live deployer maintains `deleter_deployment`
incrementally after the initial bulk resolve. The bulk join took ~13 min on a
real DB and blocked the `PartiallySynced → Syncing` flip on every restart,
making the node look perpetually "Partially synced".

Do NOT re-enable Phase 6 on the resume path. If you need to recompute
`deleter_deployment` from scratch on an already-bootstrapped node, run it as a
one-shot maintenance task, not in the boot-time orchestrator.

## Random peer selection per download — not round-robin

`deploy_remote_entity` picks a **random** starting index into the peer pool for
each concurrent download, then walks the pool on retry. Content is
CID-addressed, so any peer is equivalent for any hash; starting at index 0
collapses traffic onto one peer. A round-robin counter is not an acceptable
substitute — under high concurrency counter+modulo still produces correlated
starting points.

## Hash verification before storing — the trust boundary

Every downloaded blob is verified against the requested IPFS CID **before**
`storage.store()`. A peer returning wrong bytes for a hash is a transient
failure; the next peer is tried. This is what makes the pool model safe:
one valid peer is enough, no peer needs to be trusted. Removing the check
silently converts the model to "every peer must be trusted equally".

## 404 is per-peer, not per-hash

A 404 from one peer means "this peer doesn't have it", not "this hash doesn't
exist". Only after the entire pool 404s within the retry budget is a hash
marked permanently failed (into `failed_deployments`).

## Bounded tolerance for unparseable snapshot lines

`snapshots::parse_snapshot` tolerates a handful of unparseable lines but counts
them (`num_parse_errors`) and logs a warning. Both extremes have failed in
production: silently dropping lines makes an upstream format change look like a
successful no-op sync; failing hard on the first bad line halts sync on
cosmetic garbage.

## `/pointer-changes` `next` URLs are relative and query-only

Upstream catalysts return `next` as `?from=&to=&limit=&lastId=` — resolve it
against the **current request URL** (which still contains `/pointer-changes`),
not the bare server base. `url::Url::join` against just the server root drops
the path and the next request 404s. Regression test:
`pointer_changes::test_resolve_url_query_only_keeps_path`.

## `retry_failed` worker

Re-attempts `failed_deployments` rows on a slow interval, with its own
`peer_servers` pool (separate refresh cadence from the orchestrator). Rows
older than `RETRY_FAILED_PRUNE_TTL_DAYS` (default 7) are pruned — bootstrap-era
404s never resolve (the entity was deleted upstream years ago) and would
otherwise grow the table unbounded. Disable knob: `RETRY_FAILED_ENABLED=false`.

## Two-tier HTTP timeouts shape retry behavior

- `read_timeout` — inter-byte idle, resets per chunk: a streaming asset is
  never cut off mid-stream while bytes keep flowing.
- `timeout` — whole-request backstop against pathological trickle.

A stalled peer frees its slot in ~25 s and another catalyst is tried. This
shape is what lets multi-hundred-MB scene assets sync alongside cheap JSON
without per-route timeout tuning.

## Snapshots live in the main content store

Snapshot files are served from `/contents/<hash>` — the same store as entity
content, so clients need no second resolver. Don't split snapshots into a
separate bucket.

## Steady-state signals: frontier + heartbeat (added 2026-07-04)

"At the tip" is two different questions, answered by two `system_properties`
keys:

- `sync_frontier` (epoch ms) — **position**: the greatest pointer-changes
  timestamp fully applied. Advanced only when new deployments land.
- `sync_heartbeat` (epoch ms) — **liveness**: written after each successfully
  fetched pointer-changes page (throttled to one write per 10 s), plus at
  bootstrap-phase ends. Empty pages beat too, so a quiet network keeps
  beating with a stale frontier, while a wedged loop or a failing upstream
  stream stops beating. That asymmetry is the point — frontier lag alone
  cannot distinguish "caught up on an idle network" from "loop dead".
  Per-page (not per-call) placement matters: with `wait_time_ms > 0` the
  live pointer-changes call long-polls and **never returns** at the tip, so
  a beat on call completion would read permanently down on a healthy node
  (observed on the first prod deploy of this signal).

Both are additive: old binaries ignore the extra row; new binaries treat a
missing key as unknown (the trait default is a no-op, so non-Postgres
`DeploymentRepository` implementors are unaffected).

`GET /content/status` surfaces them as additive fields inside
`synchronizationStatus` — `syncFrontier`, `lastHeartbeat`, and `up`
(heartbeat younger than 300 s) — only when the process has sync enabled and
has beaten at least once; read-only nodes emit none of them, keeping the
payload byte-compatible with previous versions. Two caveats:
`synchronizationState` never says "Synced" (everything past bootstrap maps to
the string `Syncing`), and `lastSyncWithDAO` is backfilled with `now()` for
reference parity — use `lastHeartbeat`, not `lastSyncWithDAO`, as the
liveness signal. A paused loop (admin sync-pause) stops beating and will
read `up:false` after the threshold.

Both keys are also exported on `/metrics` as
`catalyrst_sync_{heartbeat,frontier}_timestamp_seconds` (set at write time;
present only on sync-enabled nodes after the first beat), with alert rules
`SyncHeartbeatStale` (loop dead) and `SyncIngestSilent` (beating but nothing
lands, via `catalyrst_sync_deployments_total`) in the reference NixOS module
— see [operations/observability.md](./operations/observability.md). Alert on
the heartbeat, never on the persisted frontier: the frontier row advances
only at phase ends and is legitimately stale for the whole life of a
steady-state process.

The full "are we at the tip" checklist (frontier lag + ingest rate +
upstream A/B + snapshot-hash equality) is in
[status-and-parity.md §7](./status-and-parity.md) and the SQL lives here:

```sql
SELECT key, to_timestamp(value::bigint / 1000) AS at,
       now() - to_timestamp(value::bigint / 1000) AS age
FROM system_properties WHERE key IN ('sync_frontier', 'sync_heartbeat');
```
