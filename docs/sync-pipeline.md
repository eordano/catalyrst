# Sync pipeline

catalyrst pulls deployments from a pool of upstream catalyst peers
(`SYNC_SOURCE`). Sync is fail-soft: a slow or wrong peer is dropped and
another is tried; bad bytes never poison local storage. This page
captures the load-bearing invariants from the sync code paths.

Sources:
- `crates/catalyrst-sync/src/sync_orchestrator.rs`
- `crates/catalyrst-sync/src/deploy_remote_entity.rs`
- `crates/catalyrst-sync/src/pointer_changes.rs`
- `crates/catalyrst-sync/src/snapshots.rs`
- `crates/catalyrst-sync/src/peer_cluster.rs`
- `crates/catalyrst-sync/src/retry_failed.rs`

## Phase 6 (resolve deleter_deployment) is intentionally skipped on resume

`SyncOrchestrator` runs phases 1-6 on a fresh bootstrap:

1. Bootstrap & peer discovery
2. Snapshot ingest
3. Active-entity hydration
4. Pointer-change catch-up
5. Live forward sync
6. **`resolve_deleter_deployments`** — full O(n²) self-join

Phase 6 is **deliberately skipped** when `resuming = (frontier > 0)`.

Reason: `deleter_deployment` is maintained incrementally by the live
deployer once the initial bulk resolve has been done. The bulk resolve
is an O(n²) full-table self-join that previously took ~13 min on a real
DB and was blocking the flip from `PartiallySynced` → `Syncing` on
every restart, making the node look perpetually "Partially synced".

**Do NOT re-enable Phase 6 on the resume path** without an alternative
implementation. If you need to recompute `deleter_deployment` from
scratch on a node that's already been through bootstrap, run it as a
one-shot maintenance task, not in the boot-time orchestrator.

## Download distribution across the pool

`deploy_remote_entity` picks a **random** starting server index from the
pool for each concurrent download, then walks the pool on retry.

Rationale: content is CID-addressed, so any peer is equivalent for any
hash. Starting all concurrent downloads at index 0 collapses traffic
onto a single peer and the rest of the pool sits idle. Random start
spreads load evenly.

Don't change this to a round-robin counter — under high concurrency the
counter+modulo pattern still produces correlated starting points; true
random distributes better.

## Hash verification before storing — trust boundary

Every downloaded blob is verified against the requested IPFS CID
**before** it's passed to `storage.store()`. A faulty or malicious peer
that returns the wrong bytes for a given hash is treated as a transient
failure and the next server is tried.

This is fail-closed: a peer cannot poison local storage with bad bytes
under a hash it doesn't actually own. Removing the check turns the sync
pool from "any one valid peer is enough" into "every peer in the pool
must be trusted equally."

## 404 fall-through (not a permanent failure)

A 404 from one peer for a given hash is treated as "this peer doesn't
have it", **not** as "this hash doesn't exist." The retry loop tries
another peer. A hash is only marked permanently failed after the entire
pool 404s on it within the configured retry budget.

## Bounded tolerance for unparseable snapshot lines

`snapshots::parse_snapshot` accepts a handful of unparseable lines but
counts them and surfaces them via `num_parse_errors` plus a warning log.

Why: a snapshot whose format the parser no longer understands would
otherwise look like a successful no-op sync. The visible counter + log
warning is the guard against silent data loss when upstream changes the
snapshot file format.

Don't silently drop unparseable lines, and don't fail-hard on the first
one — both modes have failed in production.

## `next` URL resolution for `/pointer-changes`

Upstream catalysts return **relative, query-only** `next` URLs in
`pointer-changes` responses, like:

```
?from=&to=&limit=&lastId=
```

These must be resolved against the *current request URL* (which still
has `/pointer-changes` in the path), not against the bare server base.
`url::Url::join` against just `server` drops the path component and the
next request 404s.

Regression test: `pointer_changes::test_resolve_url_query_only_keeps_path`.

## `retry_failed` worker

A background worker re-attempts entries in `failed_deployments` on a
slow interval. It:

- Shares storage, deployer, and failed-store with the orchestrator.
- Holds its own `peer_servers` RwLock (separate refresh cadence from
  the orchestrator's pool).
- Prunes entries older than `RETRY_FAILED_PRUNE_TTL_DAYS` (default 7).

The TTL prune exists because bootstrap-era 404s never resolve (the
entity was deleted from the source years ago). Without pruning,
`failed_deployments` grows unbounded.

Disable knob: `RETRY_FAILED_ENABLED=false`.

## HTTP client timeouts shape sync retry

- `read_timeout` = inter-byte idle (resets per chunk). Streaming assets
  are never cut off mid-stream as long as the peer keeps sending bytes.
- `timeout` = whole-request backstop against pathological trickle.

A stalled peer frees the slot in ~25 s and we retry against another
catalyst. The two-tier timeout shape is what lets large content
(scenes with big binary assets) sync alongside cheap JSON metadata
without manual per-route timeout tuning.

## Snapshots live in the main content store

Snapshot files are served from `/contents/<hash>` — same store as
entity content. This lets clients that already have the content
endpoint use snapshots without a second resolver. Don't split snapshots
into a separate bucket.
