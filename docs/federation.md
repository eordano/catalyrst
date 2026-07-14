# Federation

Federation is catalyrst-peer-to-peer - it never writes back to Decentraland servers. `FED_GOSSIP` unset = `NoopPublisher` (local writes apply, persist to Postgres, no peers).

## Write contract

```
sign -> verify (signature, session delegation, clock skew)
     -> authority check -> apply, persist (Postgres)
     -> publish GossipEnvelope (best-effort)
     -> peers re-verify, apply, or fall back to snapshot-pull
```

Upstream community/social mutations: signed-fetch headers, plain JSON body, `204`/`201 {data:...}`. Catalyrst federated mutations: EIP-712 `Signed<T>` JSON envelope in the body, `200 {ok:true, signature_hash, ...}` - the envelope travels with the payload for peer replay (signed-fetch is request-bound, ungossipable). Community write routes share one path: a body carrying `domain`+`message`+`signature` -> federation path (verify, append to federation log, gossip); anything else -> client-compat (`handlers/client/`): stock-explorer signed-fetch, upstream body shapes, parity responses. Client-compat writes are node-local: never in the federation log, so neither gossip nor snapshot-pull propagates them.

## Envelope/trust model

`GossipEnvelope` (`crates/catalyrst-fed/src/gossip.rs`): verbatim `Signed<T>` JSON, routing metadata (scope, primary type, `signature_hash`, recovered signer, origin peer). Receivers re-run local-write verification on the inner `Signed<T>`; dedup on `signature_hash` makes replays idempotent no-ops. Peer list: TOML loaded by `FederationRegistry` (`crates/catalyrst-fed/src/peer.rs`) - `peer_id`, `catalyst_url`, `gossip_pubkey`, `mtls_root_pem`, plus a required `dao_proposal`, `added_at` audit trail per entry.

## Two transports

- Disabled: `NoopPublisher` - publish no-op, no apply loop.
- NATS JetStream, behind the `nats` cargo feature (default build omits `async-nats`): subjects `fed.<scope>.actions`, one durable per-scope stream (`FED_<SCOPE>`), 30-day catch-up window; Postgres stays the durable source of truth.

`FED_GOSSIP=nats` on a binary without the feature logs a warning, falls back to no-op. Places, communities publish and consume gossip (`fed.places.actions`, `fed.communities.actions`); friends/messaging scopes declared, no writers yet.

## Environment (`GossipConfig::from_env`)

| Var | Meaning |
|---|---|
| `FED_GOSSIP` | `nats` enables live gossip; anything else/unset = disabled |
| `FED_NATS_URL` | broker URL (default `nats://127.0.0.1:4222`) |
| `FED_PEER_ID` | peer id; stamps `origin_peer`, names the durable consumer; must match this node's peer-list entry |
| `FED_NATS_CLIENT_CERT` / `_KEY` / `FED_NATS_ROOT_CA` | mTLS for federation NATS; only one of CERT/KEY set = hard error; none = plaintext, warning (loopback only) |

## Snapshot-pull catch-up

Gossip is best-effort; HTTP snapshot-pull is the durable reconciliation - after downtime, and when a NATS publish fails. Per scope (`crates/catalyrst-fed/src/snapshot.rs`):

- `GET /federation/<scope>/snapshot` - high-watermark (`latest_*_seq`) per append-only log;
- `GET /federation/<scope>/changes?since=<seq>` - applied rows, ascending, paged.

Communities expose a content-addressed blob store for federated media (thumbnails) at `/federation/communities/content[/{hash}]`: hash-verified, size-capped PUTs, a GC endpoint.

A catching-up peer pages `changes`, advances its cursor to the max `seq`, re-verifies, applies each row (dedup makes overlap a no-op), stops at the watermark; the cursor never regresses. Unit test: `snapshot.rs::reconciliation_loop_pages_to_watermark_and_dedups`.

## Verifying two nodes converge

1. Broker: `nats --server "$FED_NATS_URL" stream ls` lists `FED_PLACES` (created lazily).
2. Boot log `places gossip consumer started (fed.places.actions)`; `consumer not started ...` = gossip off (check `FED_GOSSIP`, the `--features nats` build).
3. Live push: signed write on A appears on B in ~1 s; no self-echo re-consume (filtered on `origin_peer`).
4. Catch-up: stop B, write on A, restart B - reconciles via `changes?since=`; watermarks converge.
5. Quiescent: equal `latest_*_seq` on both; re-pull applies zero rows.

Test coverage (in-process `tests/gossip_loop.rs`, env-gated `nats_live`): [build-and-test.md](./build-and-test.md).
