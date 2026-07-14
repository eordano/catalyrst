# LAND publish + unpublish (content core)

Status: IMPLEMENTED server-side (catalyrst-server, catalyrst-validator,
catalyrst-worlds); live behavior gated on applying migration 0003 and bumping
the :5141 binary (restart request). Binding design:
`~/one/docs/land-publish-design.md`.

## Publish

`POST /content/entities` (already mounted; nginx `01-catalyst.conf` proxies
`/content/` to `cat_content` :5141) accepts a catalyst-standard multipart
scene deployment. Nothing new at the edge; the only publish-side additions:

- Auth rule per pointer parcel `(x,y)`, all legs against the LOCAL indexers,
  fail-closed:
  1. parcel owner via `squid_marketplace.parcel.owner_id`;
  2. containing estate's owner via `parcel.estate_id -> estate.owner_id`;
  3. operator / updateOperator / updateManagers / approvedForAll via the
     land-manager subgraph (`handlers/external_graph.rs::parcel_operators`),
     injected into `SquidBlockchainChecker` as a `LandOperatorResolver`
     (`catalyrst-validator/src/squid_checker.rs`). A squid error denies; an
     operator-resolver error denies only the operator leg — owners are never
     locked out by a subgraph outage.
- Provenance: when the deploy context is `LOCAL` and the entity is a scene,
  `WriteDeployer::persist` records `local_entities(entity_id, signer)`
  (migration `0003_local_provenance.sql`). Missing table (migration not yet
  applied) logs a warning and skips — deploys keep working.
- The `/deployments` response cache is flushed on every accepted deploy; the
  in-process entity cache refreshes through the existing `new_deployment`
  pg_notify trigger.

Precedence is the plain catalyst happened-before order (`entity_timestamp`,
lowercase `entity_id` tiebreak): a fresh local publish is newest and wins at
serve time immediately; a strictly newer upstream Genesis City sync later
supersedes it (the `local_entities` row survives and audit shows
`"status": "superseded"`); local rows never block sync ingestion.

## Unpublish

`DELETE /content/scenes/{x},{y}` (also mounted at `/scenes/{x},{y}`), behind
the same read-only gate as deploys.

- Auth: signed fetch (`x-identity-auth-chain-*` headers, worlds-style; ported
  as `catalyrst-server/src/signed_fetch.rs`), then the same LAND rule for the
  addressed parcel.
- Precondition: the pointer's active entity must be a non-tombstoned local
  publish, else `404 No locally published scene at {x},{y}.` Synced Genesis
  City entities are never deletable here.
- Effect (`land_publish::tombstone_and_repoint`, one tx under the same
  per-pointer advisory locks as `persist`): sets
  `local_entities.tombstoned_at`; clears `deleter_deployment` on rows the
  tombstoned entity had overwritten (so the restored upstream row serves
  again); for each pointer held by the entity repoints `active_pointers` to
  the newest non-tombstoned deployment covering it, or deletes the pointer
  row when none exists; emits `pg_notify('new_deployment', 'scene:<id>')` so
  the in-process entity cache drops the tombstoned entry (the refresh query
  now excludes tombstoned locals). `deployments` history rows are never
  deleted.
- Responses: `200 {"entityId", "unpublished": true, "parcels": [...]}`,
  `401/400` on bad signatures, `403` on the LAND rule, `404` on the
  precondition, `409` when a concurrent deployment raced the tombstone.

## Audit surface

`GET /content/audit/{type}/{entityId}` gains `localProvenance` when the
entity was published locally:

```json
{ "signer": "0x...", "origin": "land-publish", "publishedAt": 1758000000000,
  "tombstonedAt": null, "superseded": false, "status": "active" }
```

`status` is `active` | `superseded` (a newer Genesis City deployment replaced
it) | `unpublished`.

## Worlds parity route

catalyrst-worlds now serves `DELETE /entities/{world_name}` (upstream
worlds-content-server parity): signed fetch, owner or world-wide `deployment`
permission, undeploys every scene of the world, returns `200 {}`. This fixes
the pre-deploy delete that stock CLIs (and dcl-one-sdk `send_world_delete`)
issue against this node. LAND deploys never hit this path.

## Edge cap (staged, human-gated)

nginx caps `client_max_body_size` at 25M for the catalyst server block while
the service accepts 50M per file. The staged override lives in the
deployment's nginx conf.d as `land-publish-entities-bodysize.staged` (inert:
only `*.conf` is included) — applying it is an ops call.

## Tests

- `catalyrst-server/tests/land_publish.rs` — env-gated Postgres suite
  (`CATALYRST_SERVER_TEST_PG`): provenance states, tombstone repointing (to
  upstream, to nothing, skipping tombstoned locals), synced/double-unpublish
  refusals, the squid owner/estate/operator/fail-closed matrix, and a full
  in-process publish→deny-stranger→unpublish roundtrip through
  `WriteDeployer` with real signatures.
- `catalyrst-server/src/signed_fetch.rs` unit tests — fresh/stale/wrong-path/
  missing-chain signed fetches.
- `catalyrst-validator` unit tests — operator-leg grant matrix.
