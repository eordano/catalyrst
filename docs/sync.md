# Sync pipeline - load-bearing invariants

catalyrst pulls deployments from a pool of upstream catalyst peers
(`SYNC_SOURCE`); fail-soft - a slow or wrong peer is dropped and another
tried, bad bytes never poison local storage. Sources:
`crates/catalyrst-sync/src/{sync_orchestrator,deploy_remote_entity,pointer_changes,snapshots,peer_cluster,retry_failed}.rs`.

## Phase 6 (resolve `deleter_deployment`) is skipped on resume - on purpose

Bootstrap phases: peer discovery, snapshot ingest, active-entity hydration,
pointer-change catch-up, live forward sync, `resolve_deleter_deployments`
(O(n^2) full-table self-join). On resume (`frontier > 0`) Phase 6 is
skipped: the live deployer maintains `deleter_deployment` incrementally; the
bulk join took ~13 min on a real DB and blocked the
`PartiallySynced -> Syncing` flip on every restart. Do NOT re-enable it on
the resume path - recompute only as a one-shot maintenance task.

## Download and verification rules

- Random peer start index per download (`deploy_remote_entity`), retry walks
  the pool; index 0 collapses traffic onto one peer, and a round-robin
  counter still correlates starting points under high concurrency.
- Every blob is verified against the requested IPFS CID before
  `storage.store()`; wrong bytes = transient failure, next peer tried. One
  valid peer is enough; removing the check means trusting every peer.
- 404 is per-peer, not per-hash: a hash is marked permanently failed
  (`failed_deployments`) only after the whole pool 404s in the retry budget.
- `snapshots::parse_snapshot` tolerates a few unparseable lines, counts them
  (`num_parse_errors`), warns. Silent drops hide upstream format changes;
  failing hard halts sync on cosmetic garbage.
- `/pointer-changes` `next` URLs are relative and query-only
  (`?from=&to=&limit=&lastId=`): resolve against the current request URL,
  not the bare server base - `url::Url::join` against the root drops the
  path and 404s. Test:
  `pointer_changes::test_resolve_url_query_only_keeps_path`.
- `retry_failed` worker re-attempts `failed_deployments` rows on a slow
  interval with its own `peer_servers` pool, `SYNC_RETRY_CONCURRENCY`
  (default 10) rows in flight per pass; rows older than
  `RETRY_FAILED_PRUNE_TTL_DAYS` (default 7) are pruned (bootstrap-era 404s
  never resolve). Disable knob: `RETRY_FAILED_ENABLED=false`.
- Two-tier HTTP timeouts: `read_timeout` (inter-byte idle, resets per chunk)
  plus `timeout` (whole-request backstop); a stalled peer frees its slot in
  ~25 s, so huge scene assets sync alongside cheap JSON without tuning.
- Snapshot files are served from `/contents/<hash>` - the main content
  store; don't split snapshots into a separate bucket.

## Steady-state signals: frontier + heartbeat

Two `system_properties` keys, both additive (old binaries ignore the row;
missing key = unknown; trait default no-op, so non-Postgres
`DeploymentRepository` implementors are unaffected):

- `sync_frontier` (epoch ms) - position: greatest pointer-changes timestamp
  fully applied; advances only when new deployments land.
- `sync_heartbeat` (epoch ms) - liveness: written per successfully fetched
  pointer-changes page (throttled to one write per 10 s) plus at
  bootstrap-phase ends. Empty pages beat: quiet network = beating with a
  stale frontier, wedged loop = silent. Per-page (not per-call) placement:
  with `wait_time_ms > 0` the live call long-polls and never returns at the
  tip, so a completion-time beat reads permanently down on a healthy node
  (observed on first prod deploy).

`GET /content/status` adds `syncFrontier`, `lastHeartbeat`, `up` (heartbeat
younger than 300 s) inside `synchronizationStatus`, only once sync is
enabled and has beaten; read-only nodes emit none (payload stays
byte-compatible). Caveats: `synchronizationState` never says "Synced"
(post-bootstrap = `Syncing`); `lastSyncWithDAO` is backfilled with `now()` -
use `lastHeartbeat` for liveness. A paused loop (admin sync-pause) reads
`up:false` after the threshold.

`/metrics` exports `catalyrst_sync_{heartbeat,frontier}_timestamp_seconds`
(set at write time; present on sync-enabled nodes after the first beat);
alert rules `SyncHeartbeatStale` (loop dead) and `SyncIngestSilent` (beating
but nothing lands, via `catalyrst_sync_deployments_total`) ship in the
reference NixOS module - see
[operations/observability.md](./operations/observability.md). Alert on the
heartbeat, never the persisted frontier (advances only at phase ends;
legitimately stale in steady state).

At-the-tip checks: frontier lag + ingest rate + upstream A/B + snapshot-hash
equality.

```sql
SELECT key, to_timestamp(value::bigint / 1000) AS at,
       now() - to_timestamp(value::bigint / 1000) AS age
FROM system_properties WHERE key IN ('sync_frontier', 'sync_heartbeat');
```
