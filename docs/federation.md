# Federation - signed writes, gossip, snapshot-pull

Federation is catalyrst-peer-to-peer only - it never writes back to real
Decentraland servers. `FED_GOSSIP` unset = `NoopPublisher` (local writes
apply + persist to Postgres, no peers).

## Write contract - deliberate divergence from the stock client

```
sign -> verify (signature + session delegation + clock skew)
     -> authority check -> apply + persist (Postgres)
     -> publish GossipEnvelope (best-effort)
     -> peers re-verify + apply, or fall back to snapshot-pull
```

Upstream community/social mutations: signed-fetch headers + plain JSON body,
`204`/`201 {data:...}`. catalyrst federated mutations: EIP-712 `Signed<T>`
JSON envelope in the body, `200 {ok:true, signature_hash, ...}` - the
envelope travels with the payload so peers can re-verify and replay it
(signed-fetch binds to one HTTP request, cannot be gossiped). The community
write routes serve both on the same paths: a JSON body carrying
`domain`+`message`+`signature` is dispatched to the federation path
(verified, appended to the federation log, gossiped); any other body goes to
the client-compat path (`handlers/client/`) - stock-explorer signed-fetch
writes with upstream body shapes and upstream-parity responses. Client-compat
writes apply to this node only: they never enter the federation log, so
neither gossip nor snapshot-pull propagates them to peers.

## Envelope + trust model

`GossipEnvelope` (`crates/catalyrst-fed/src/gossip.rs`): the verbatim
`Signed<T>` JSON plus routing metadata (scope, primary type,
`signature_hash`, recovered signer, origin peer). Receivers re-run the full
local-write verification on the inner `Signed<T>`; dedup on
`signature_hash`, replays are idempotent no-ops. Peers are declared in a
peer-list TOML loaded by `FederationRegistry`
(`crates/catalyrst-fed/src/peer.rs`): `peer_id`, `catalyst_url`,
`gossip_pubkey`, `mtls_root_pem`, plus a required `dao_proposal` +
`added_at` audit trail per entry.

## Two transports

- Disabled (default) - `NoopPublisher`: publish no-op, no apply loop.
- NATS JetStream - behind the `nats` cargo feature (default build omits
  `async-nats`). Subjects `fed.<scope>.actions`; one durable per-scope
  stream (`FED_<SCOPE>`), 30-day catch-up window; Postgres remains the
  durable source of truth.

`FED_GOSSIP=nats` on a binary built without the feature logs a warning and
falls back to no-op. Places and communities both publish and consume live
gossip when enabled (`fed.places.actions`, `fed.communities.actions`);
friends/messaging scopes are declared but have no writers yet.

## Environment (parsed by `GossipConfig::from_env`)

| Var | Meaning |
|---|---|
| `FED_GOSSIP` | `nats` enables live gossip; anything else/unset = disabled |
| `FED_NATS_URL` | broker URL (default `nats://127.0.0.1:4222`) |
| `FED_PEER_ID` | stable peer id; stamps `origin_peer`, names the durable consumer; must match this node's peer-list entry |
| `FED_NATS_CLIENT_CERT` / `_KEY` / `FED_NATS_ROOT_CA` | mTLS for the federation NATS account. Setting only one of CERT/KEY is a hard error; none set = plaintext + warning (acceptable only on loopback) |

## Snapshot-pull catch-up (the always-correct path)

Gossip is best-effort; HTTP snapshot-pull is the durable reconciliation
path: after downtime, and when a NATS publish fails (the action is already
durable in Postgres). Per scope (`crates/catalyrst-fed/src/snapshot.rs`):

- `GET /federation/<scope>/snapshot` - high-watermark (`latest_*_seq`) per
  append-only log;
- `GET /federation/<scope>/changes?since=<seq>` - applied rows, ascending,
  paged.

Communities additionally expose a content-addressed blob store for federated
media (thumbnails) at `/federation/communities/content[/{hash}]` - PUTs are
hash-verified and size-capped, plus a GC endpoint.

A catching-up peer pages `changes`, advances its cursor to the max `seq`
returned, re-verifies + applies each row (dedup makes overlap a no-op),
stops at the watermark; the cursor never regresses. Unit-tested in
`snapshot.rs` (`reconciliation_loop_pages_to_watermark_and_dedups`).

## Verifying two nodes converge

1. Broker: `nats --server "$FED_NATS_URL" stream ls` lists `FED_PLACES`
   (created lazily).
2. Boot log `places gossip consumer started (fed.places.actions)`; the
   `consumer not started ...` variant = gossip off (check `FED_GOSSIP` and
   the `--features nats` build).
3. Live push: signed write on A appears on B within ~1 s; no self-echo
   re-consume (filtered on `origin_peer`).
4. Catch-up: stop B, write on A, restart B -> B reconciles via
   `changes?since=`; watermarks converge.
5. Quiescent: equal `latest_*_seq` on both; re-pull applies zero rows.

The publish->consume->re-verify->apply->dedup loop is covered in-process (no
broker) by `tests/gossip_loop.rs`; env-gated `nats_live` uses a real broker,
skips when `FED_NATS_URL` is unset (CI stays broker-free).
