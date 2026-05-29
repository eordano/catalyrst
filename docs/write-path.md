# Write path (`POST /content/entities`)

The write path is opt-in via `ENABLE_DEPLOYMENTS=true`. When disabled,
the server installs a fail-closed read-only deployer; the route is
removed entirely from the router (`crates/catalyrst-server/src/routes.rs`
gates on `!read_only`). When enabled, the route mounts at
`POST /content/entities` with a 200 MiB body cap.

This page documents the load-bearing invariants of the deploy validator
pipeline that lives behind that route.

Sources:
- `crates/catalyrst-server/src/write_deployer.rs`
- `crates/catalyrst-server/src/handlers/create_entity.rs`
- `crates/catalyrst-deployer/src/deployment_service.rs`

## Top-level decision flow

```
POST /content/entities (multipart)
    │
    ├── bootstrap guard         → 503 if state == Bootstrapping
    ├── parse multipart
    ├── auth-chain verify       → 401 if missing/invalid
    ├── EIP-1654 (if applicable)→ 401/500 on RPC error (fail-closed)
    ├── entity structure        → 400 on bad shape
    ├── content hash check      → 400 on mismatch
    ├── ADR-45 / IPFS hashing   → 400 on mismatch
    ├── per-type size limit     → 413
    ├── third-party Merkle proof → 401 on bad proof
    ├── on-chain access check   → 401 if !IGNORE_BLOCKCHAIN_ACCESS_CHECKS
    ├── request TTL window      → 400 outside [-20m, +15m]
    ├── newer-entity check      → 409 if a newer entity exists on these pointers
    ├── per-type rate-limiter   → 429
    ├── per-profile "unchanged metadata" rate-limiter (profile-only)
    │                             → 429 if metadata == active and within 5 min
    └── commit (transactional)  → 200 + persistence with overwrite parity
```

## Fail-closed defaults

- `IGNORE_BLOCKCHAIN_ACCESS_CHECKS` defaults to **false**. An unset flag
  MUST NOT bypass on-chain ownership/access checks. Only an explicit
  `=true` enables the bypass (used for historical-profile sync only).
- Validator-to-crypto chain bridge (`validator_chain_to_crypto`) uses a
  JSON round-trip and **rejects unknown auth-chain link types** rather
  than silently dropping them. New link types must be added to the
  bridge before deploys carrying them can be accepted.
- `ETH_RPC_URL` must be HTTPS — server refuses plaintext `http://` at
  startup. See `docs/auth-chain.md` for why this is security-critical.

## `happened_before` total order is the single source of truth

Two SQL predicates govern the commit decision:

- `stored_is_newer_or_equal`: blocks the deploy when a stored row is
  newer than us.
- `stored_is_overwritten`: identifies which stored rows this deploy
  overwrites.

Both derive from the same total order: `(timestamp, lower(entity_id))`.
They are exact complements **except at identical `(ts, id)`** (the
identity case), which is neither newer-than nor overwritten-by.

The `cfg(test)` mirrors in `write_deployer.rs` check this invariant
directly against the SQL form — they exist so a change to the predicate
(e.g. adding a tiebreaker, swapping case sensitivity) can't drift
between the two query sites without a test failure.

## Case-insensitive entity_id tiebreak

All SQL predicates use `lower(entity_id)` for the tiebreak. The pure-Rust
mirrors must match. Entity IDs are hex-encoded SHA / CID strings and the
reference catalyst does a case-insensitive compare; switching to
case-sensitive would re-order identically-timestamped deploys and break
overwrite resolution.

## Stored metadata wrapping shape: `{"v": <meta>}`

Active deployments persist metadata wrapped as `{"v": <meta>}`.
`metadata_unchanged` unwraps before comparing.

A `NULL`/`None` stored metadata is treated as "changed" — never as
"equal to anything." This avoids a degenerate case where a deploy is
denied because the active pointer holds null.

## `is_content_unchanged` short-circuit is PROFILE-ONLY

Profile deploys whose metadata is byte-equal to the stored active are
dropped (200 success, no re-write). This prevents the most common
client-side bug — re-deploying an identical profile every avatar swap —
from causing unnecessary DB writes and snapshot churn.

The short-circuit is gated on entity type because scenes, wearables,
emotes can legitimately re-deploy with identical metadata (e.g.
content-only changes update the same metadata blob).

## `has_newer_entity` is a temporal guard BEFORE validation

`has_newer_entity` mirrors the reference's `areThereNewerEntitiesOnPointers`.
It runs early so the expensive validator pipeline (auth chain,
Merkle proofs, on-chain calls) never executes for deploys that will lose
to a newer entity anyway.

Keep it before validation — moving it after wastes RPC calls and burns
the rate limit.

## Rate limiter

### Per-type TTL / size buckets (the public-facing rate limit)

Reference defaults:

| entity_type | TTL  | max_size |
|-------------|------|----------|
| profile     | 3 s  | 500      |
| scene       | 20 s | 1000     |
| wearable    | 20 s | 1000     |
| store       | 3 s  | 300      |
| emote       | 20 s | 1000     |
| outfit      | 3 s  | 2000     |

A pointer is "hot" for `TTL` seconds after a deploy lands. The bucket
trips (429) once total hot pointers exceeds `max_size`.

### Profile-only "unchanged metadata" bucket

5-minute TTL, unbounded size. Prevents the avatar-swap loop from being a
DoS vector against the validator even when each deploy's metadata is
identical (and therefore short-circuited above) — the rate-limiter
counts the request, not the work.

### Recording is post-success only

Failed deploys do NOT poison the bucket. A 400 or 401 on input means
the request shouldn't count against legitimate retries.

## Request TTL window

| Direction      | Limit | Reference constant         |
|----------------|-------|----------------------------|
| backwards      | 20 min| `REQUEST_TTL_BACKWARDS`    |
| forwards       | 15 min| `REQUEST_TTL_FORWARDS`     |

Deploys outside the window are rejected (400). Forwards is shorter
because the only legitimate reason to deploy in the future is clock
skew, which should be < 15 minutes; longer windows enable backdating
attacks.

## CIDv1 for content addressing

`calculate_files_hashes` uses `hash_bytes_v1` (CIDv1) because current
entities are CIDv1-addressed. CIDv0 support is read-only for legacy
content.

## `DECENTRALAND_ADDRESS = 0x1337...` bypass

The literal `0x1337742816108ce98d0e1cc6d99fb84b30dafd03` matches the
reference `DECENTRALAND_ADDRESS`. This address bypasses some access
checks — historically used for Decentraland Foundation deploys
(e.g. base wearables) that don't fit the normal ownership model.

`ADDITIONAL_DECENTRALAND_ADDRESS` env var adds operator-controlled
extra privileged addresses. Use sparingly; any address here can deploy
to any pointer.

## Overwrite parity

On commit success:

- `overwrote` set = older rows on overlapping pointers that aren't
  already superseded by something newer than us (mirrors
  `calculateOverwrote`).
- For each overwritten deployment, `deleter_deployment` is set to this
  entity_id; pointers held by overwritten deployments that this entity
  does NOT re-claim are deleted from `active_pointers` (`pointer_manager`
  CLEARED parity).
- This deploy's `deleter_deployment` stays NULL at insert because
  `has_newer_entity` already proved nothing newer exists.

## Idempotent re-deploy

If the INSERT hits the `entity_id` unique constraint (same entity
already deployed), the transaction commits and returns success. Clients
that retry on timeout don't see a spurious 409.

## Unreferenced uploads are passed to the validator, not silently dropped

Files in the multipart body that match neither a declared content hash
nor the entity id are still passed to the validator (which rejects them
with a clear error). Silently dropping unknown files would mask a
common client bug (sending the wrong filename) as a successful deploy
with missing content.

## Legacy entity cutoff

`legacy_content_migration_timestamp_ms = 1_582_167_600_000` (2020-02-20 UTC)
is the cutoff below which a Synced/FixAttempt deployment is reclassified
as `SyncedLegacyEntity`. The 30-day backwards-TTL
(`request_ttl_backwards_ms = 30 * 24 * 60 * 60 * 1000`) is a separate
constant for catching up legacy deploys without giving recent deploys
the same generous window.

Both are parity-critical magic constants. Changing them re-classifies
historical deploys.
