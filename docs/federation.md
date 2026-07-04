# Federation — signed writes, gossip, snapshot-pull

> Status: distilled 2026-07-04 from `crates/catalyrst-fed` and the gossip
> runbook; last re-verified against code 2026-07-03 (docs-stale-audit).

Federation is **catalyrst-peer-to-peer only** — it never writes back to real
Decentraland servers. A single node needs none of it: with `FED_GOSSIP` unset
the services run on the `NoopPublisher` (local writes apply + persist to
Postgres, no peers).

## The write contract — and why it diverges from the stock client

Every federated write follows one path, local or remote:

```
sign → verify (signature + session delegation + clock skew)
     → authority check → apply + persist (Postgres)
     → publish GossipEnvelope (best-effort)
     → peers re-verify + apply, or fall back to snapshot-pull
```

**This is a deliberate divergence from the upstream client contract.** Upstream
community/social mutations are signed-fetch headers + a plain JSON body
returning `204`/`201 {data:…}`. catalyrst's federated mutations require an
EIP-712 `Signed<T>` JSON envelope in the body and return
`200 {ok:true, signature_hash, …}`. The envelope is what makes a write
re-verifiable and replayable by peers (the signature travels with the payload;
signed-fetch signatures bind to one HTTP request and cannot be gossiped).
Consequence: the stock explorer cannot perform community writes against
catalyrst today — those handlers return **501** until either a
client-compatibility adapter (accept signed-fetch, wrap into `Signed<T>`) or an
explorer-side change ships. This is the single biggest intentional
wire-contract divergence in the workspace; see
[status-and-parity.md](./status-and-parity.md).

## Envelope + trust model

The wire unit is `GossipEnvelope` (`crates/catalyrst-fed/src/gossip.rs`): the
verbatim `Signed<T>` JSON plus routing metadata (scope, primary type,
`signature_hash`, recovered signer, origin peer). A receiver **re-runs the full
local-write verification** on the inner `Signed<T>` — gossip is never trusted
because a peer forwarded it. Dedup is on `signature_hash`; a replayed envelope
is an idempotent no-op.

Peers are declared in a peer-list TOML loaded by `FederationRegistry`
(`crates/catalyrst-fed/src/peer.rs`): `peer_id`, `catalyst_url`,
`gossip_pubkey`, `mtls_root_pem`, plus a required `dao_proposal` + `added_at`
audit trail per entry.

## Two transports

- **Disabled** (default) — `NoopPublisher`; `publish` is a no-op, `subscribe`
  yields nothing, no apply loop is spawned.
- **NATS JetStream** — compiled in behind the `nats` cargo feature (the default
  build omits `async-nats`). Subjects `fed.<scope>.actions`; one durable
  per-scope stream (`FED_<SCOPE>`) gives a 30-day catch-up window. Postgres
  remains the durable source of truth.

If `FED_GOSSIP=nats` is set on a binary built **without** the feature, the
service logs a warning and falls back to no-op rather than refusing to boot.

> Communities deliberately federate via snapshot-pull only and stay on the
> `NoopPublisher` even when gossip is enabled elsewhere. Places (and later
> friends/messaging) use live gossip.

## Environment (parsed by `GossipConfig::from_env`)

| Var | Meaning |
|---|---|
| `FED_GOSSIP` | `nats` enables live gossip; anything else/unset = disabled |
| `FED_NATS_URL` | broker URL (default `nats://127.0.0.1:4222`) |
| `FED_PEER_ID` | stable peer id; stamps `origin_peer`, names the durable consumer; must match this node's peer-list entry |
| `FED_NATS_CLIENT_CERT` / `_KEY` / `FED_NATS_ROOT_CA` | mTLS for the federation NATS account. Setting only one of CERT/KEY is a hard error; none set = plaintext + warning (acceptable only on loopback) |

## Snapshot-pull catch-up (the always-correct path)

Live gossip is best-effort; HTTP snapshot-pull is the durable reconciliation
path — after downtime, for communities always, and when a NATS publish fails
(the action is already durable in Postgres). Per scope
(`crates/catalyrst-fed/src/snapshot.rs`):

- `GET /federation/<scope>/snapshot` — current high-watermark
  (`latest_*_seq`) per append-only log;
- `GET /federation/<scope>/changes?since=<seq>` — applied rows, ascending,
  paged.

A catching-up peer pages `changes` advancing its cursor to the max `seq`
returned, re-verifies + applies each row (dedup makes overlap a no-op), and
stops at the watermark. The cursor never regresses on a stale page; the loop
terminates. Unit-tested in `snapshot.rs`
(`reconciliation_loop_pages_to_watermark_and_dedups`).

## Verifying two nodes converge

1. Broker reachable: `nats --server "$FED_NATS_URL" stream ls` lists
   `FED_PLACES` (created lazily).
2. Boot log says `places gossip consumer started (fed.places.actions)`; the
   alternative message (`consumer not started …`) means gossip is off — check
   `FED_GOSSIP` and the `--features nats` build.
3. Live push: a signed write on A appears on B within ~1 s; A does not
   re-consume its own write (self-echo filtered on `origin_peer`).
4. Catch-up: stop B, write on A, restart B → B reconciles via
   `changes?since=`; watermarks converge.
5. Quiescent convergence: equal `latest_*_seq` on both; re-pull applies zero
   rows.

The full publish→consume→re-verify→apply→dedup loop is covered in-process (no
broker) by `tests/gossip_loop.rs`; the env-gated `nats_live` test exercises a
real broker and skips when `FED_NATS_URL` is unset, so CI stays broker-free.
