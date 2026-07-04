# catalyrst-communities — TODO

The federation write path is implemented as of 2026-05-30. All 17 signed
write handlers accept `Signed<T>` payloads, verify signature + outer
auth-chain + nonce-replay + rate-limit + authority chain, and persist to
the federation log tables. The legacy upstream-shaped read tables are
maintained as a materialised projection by the same apply path.

## Implemented

1. **Schema** — `migrations/0002_federation.sql` adds `communities_local`,
   `community_role_log`, `community_role_current` (with trigger), per-table
   `*_log` tables for posts / likes / places / requests, and `seen_nonces`.
   The trigger on `community_role_log` resolves the
   `(signed_at, signature_hash)` tiebreaker per the federation specification.

2. **Signed actions** — every type in `src/fed_messages.rs` carries a
   deterministic `TypedMessage::encode_struct`. JSON-roundtrip + byte-equality
   verified in `tests/federation_smoke.rs::signed_message_roundtrips_bytes`.

3. **Replay** — `src/fed/replay.rs` (in-process LRU + DB-backed `seen_nonces`
   for restart durability). 5-minute past skew enforced via
   `MAX_SKEW_PAST_SECS`; future skew via `MAX_SKEW_FUTURE_SECS`.

4. **Authority** — `src/fed/authority.rs` enforces the role hierarchy:
   owner > admin > mod > member > banned, with `can_grant` matching the
   federation specification.

5. **Wire-up** — all 17 federation-modelled handlers in
   `src/handlers/writes.rs` validate, gate, and persist. The 18th (admin
   batch read) is bearer-token-gated, no Signed<T>.

6. **Federation read endpoints** — `GET /federation/communities/snapshot`
   and `GET /federation/communities/changes?since=&limit=`, registered in
   `main.rs`.

7. **Test coverage** — 6 integration tests under `tests/federation_smoke.rs`
   covering create→join→role→ban, tiebreaker determinism, replay, hash
   determinism, byte-roundtrip, and authority gate. 9 more under
   `tests/content_store.rs` cover the hash-addressed body store
   (roundtrip, idempotent PUT, 256 KiB cap, missing → None, hash mismatch,
   GC drops orphans) plus end-to-end PUT → sign CommunityPost → apply +
   "post arrives before body" (apply must NOT block on local presence).

8. **Content store for post bodies.** Filesystem-backed hash-addressed
   store at `src/content_store.rs`, configured by `COMMUNITIES_CONTENT_DIR`
   (default `<DATA_DIR>/communities/content`). Layout:
   `<base>/<prefix2>/<prefix4>/<sha256-hex>`. PUT body cap is 256 KiB
   (413 on overflow). Endpoints:
   - `POST /federation/communities/content` — accepts raw body, returns
     `{ok, content_hash, size}`.
   - `GET /federation/communities/content/{hash}` — streams raw bytes,
     `application/octet-stream`, 404 when missing.
   - `POST /federation/communities/content/gc` — admin-only (bearer
     `API_ADMIN_TOKEN`). Walks the content dir and deletes any file whose
     hex is not referenced by `community_posts_log.content_hash`.
   `create_post` logs a debug message when a signed post arrives whose
   body is not yet local, but does NOT make local presence a precondition
   of acceptance — that would break the federation invariant that posts
   can arrive before their bodies.

## Deferred to v2

These were on the original blocker list but are intentionally out of scope
for the v1 federation write path:

- **NATS JetStream gossip.** v1 federates by HTTP snapshot+changes pull
  only. The federation specification's NATS subjects in
  `catalyrst_fed::gossip` remain stubs.
  When the broker lands, the subscriber wires onto the same apply path.
- **Content store for post bodies.** Done — see #8 above. Storage is local
  to this crate (filesystem hash dir), and GC is admin-only. Federation
  pull from peer catalysts (push-from-other-peer-on-miss) remains v2.
- **`FederationRegistry` on-chain peer set.** v1 reads peers from a
  config-file path via `catalyrst_fed::peer::FederationRegistry::load_static`.
  On-chain `FederationRegistry.sol` read via alloy is deferred.
- **`CommunityRequest` signed action.** The join-request flow is not yet
  modelled as a `Signed<T>` action; `POST /v1/communities/{id}/requests`
  returns 501 with a deferred-action body. The accept/reject side
  (`PATCH .../requests/{requestId}`) IS implemented via
  `CommunityRequestStatusUpdate`.
- **Equivocation slashing.** Detected (lower-sig wins) but not punished.
- **MLS-encrypted community posts.** Flag-gated in `CommunityCreate.flags`
  but no MLS group state is plumbed in this crate.
- **`community_posts.content_hash` migration.** Legacy `community_posts.content`
  column is reused to store the content_hash string (UTF-8 hex) on apply.
  Renaming to `content_hash` is a 0003 follow-up once any downstream
  read-path consumer is aware.
- **Admin batch read on `POST /v1/members/{address}/communities`.** Bearer-
  gated route returns 501 pending the federation-aware projection.

## Schema decisions

- **0001 tables kept as projection.** `communities`, `community_members`,
  `community_posts`, `community_places`, `community_bans` are maintained
  by the federation apply path (see `src/fed/apply.rs`). The bridge from
  hex community_id to UUID is `community_uuid_from_hex(hex)` —
  first 16 bytes of the SHA-256 hash, RFC-4122 v4 variant bits set.
- **`seen_nonces.expires_at`** is `signed_at + MAX_SKEW_PAST_SECS`; the
  startup hook in `Replay::new` GCs stale rows before loading the LRU.
- **`*_log.seq`** is a BIGSERIAL on every log table — used by
  `/federation/communities/changes?since=<seq>` for monotonic pull.

## Crate hygiene

- No source comments (workspace standing preference). Only this TODO
  and ROUTES.md hold prose.
