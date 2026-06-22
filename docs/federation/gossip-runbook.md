# Federation gossip — operator runbook

How to enable multi-node catalyrst federation: live NATS gossip for sub-second
push, with HTTP snapshot-pull as the always-available catch-up path. Federation
is **catalyrst-peer-to-peer only** — it never writes back to real Decentraland
servers.

A single node needs none of this: with `FED_GOSSIP` unset the services run on
the `NoopPublisher` (local writes apply + persist to Postgres, no peers). Turn
the knobs below on only when you actually have peers to converge with.

---

## 1. How it works

Every federated write follows one path, whether it originates locally or arrives
from a peer:

```
sign  ->  verify (signature + session delegation + clock skew)
      ->  authority check  ->  apply + persist (Postgres)
      ->  publish GossipEnvelope (best-effort)
      ->  peers re-verify + apply, or fall back to snapshot-pull
```

The wire unit is a `GossipEnvelope` (`crates/catalyrst-fed/src/gossip.rs`): the
verbatim `Signed<T>` JSON plus routing metadata (scope, primary type,
`signature_hash`, recovered signer, origin peer). A receiver **re-runs the full
local-write verification** on the inner `Signed<T>` — gossip is never trusted
because a peer forwarded it. Dedup is on `signature_hash`, so a replayed
envelope is an idempotent no-op.

Two transports, selected at runtime by `GossipConfig`:

- **Disabled** (default) — `NoopPublisher`. Single-node, or multi-node where
  peers reconcile purely via snapshot-pull. `publish` is a no-op; `subscribe`
  yields nothing, so no apply loop is spawned.
- **Nats** — real JetStream publish/subscribe, compiled in behind the `nats`
  cargo feature. Subjects are `fed.<scope>.actions`; one durable per-scope
  stream (`FED_<SCOPE>`) gives a 30-day catch-up window for briefly-offline
  peers. Postgres remains the durable source of truth.

> Communities deliberately federate via snapshot-pull only and stay on the
> `NoopPublisher` even with gossip enabled elsewhere. Places (and later
> friends/messaging) use live gossip.

---

## 2. Build with the `nats` feature

The default build omits `async-nats` so the workspace compiles where the broker
crate is unavailable. To get the live transport, build with the feature:

```bash
dcl-shell -c "cargo build --release -p catalyrst-fed --features nats"
# ...and the same `--features nats` on whichever service binary embeds fed.
```

If you set `FED_GOSSIP=nats` against a binary built **without** the feature, the
service logs a warning and falls back to the no-op (snapshot-pull only) rather
than failing to boot.

---

## 3. Enable gossip — environment

Set these on each catalyst that should push/consume live (parsed by
`GossipConfig::from_env`):

| Var | Required | Meaning |
|---|---|---|
| `FED_GOSSIP` | yes | `nats` to enable live gossip; anything else / unset = disabled |
| `FED_NATS_URL` | yes (for nats) | broker URL, e.g. `nats://broker.example:4222` (default `nats://127.0.0.1:4222`) |
| `FED_PEER_ID` | yes (for nats) | this catalyst's stable peer id; stamps `origin_peer` and names the durable JetStream consumer. Must match this node's entry in the peer list |

mTLS for the federation NATS account (set all three to peer remotely; omit all
three only for a single-broker loopback dev deploy):

| Var | Meaning |
|---|---|
| `FED_NATS_CLIENT_CERT` | path to this catalyst's PEM client cert |
| `FED_NATS_CLIENT_KEY` | path to the matching PEM private key |
| `FED_NATS_ROOT_CA` | path to the CA root (the peers' `mtls_root_pem`) |

Setting only one of CERT/KEY is a hard error. With none set the connect is
plaintext and logs a warning — acceptable only on loopback. A catalyst peering
with a *remote* node without mTLS is insecure and should be left out of the peer
list at validation time.

Peers themselves are declared in the peer-list TOML loaded by
`FederationRegistry` (`crates/catalyrst-fed/src/peer.rs`): each entry carries
`peer_id`, `catalyst_url`, `gossip_pubkey`, `mtls_root_pem`, and a required
`dao_proposal` + `added_at` audit trail.

Example unit env:

```ini
FED_GOSSIP=nats
FED_NATS_URL=nats://broker.internal:4222
FED_PEER_ID=interconnected.online
FED_NATS_CLIENT_CERT=/etc/catalyrst/fed/client.pem
FED_NATS_CLIENT_KEY=/etc/catalyrst/fed/client.key
FED_NATS_ROOT_CA=/etc/catalyrst/fed/ca.pem
```

---

## 4. Snapshot-pull catch-up

Live gossip is best-effort; the durable, always-correct reconciliation path is
HTTP snapshot-pull. It is how a node catches up after downtime, how communities
federate at all, and the fallback when a NATS publish fails (the action is
already durable in Postgres).

Each scope exposes two endpoints (paths from
`crates/catalyrst-fed/src/snapshot.rs`):

- `GET /federation/<scope>/snapshot` — returns the current high-watermark
  (`latest_*_seq`) per append-only log.
- `GET /federation/<scope>/changes?since=<seq>` — returns applied rows with
  `seq > since`, ascending, paged.

A catching-up peer:

1. reads the snapshot watermark for each log,
2. pages `changes?since=<cursor>`, advancing `cursor` to the max `seq` returned,
3. re-verifies + applies each row (dedup on `signature_hash` makes re-pulled
   overlap a no-op),
4. stops once `cursor >= watermark`.

The cursor never regresses on a stale page, and the loop terminates at the
watermark. This contract is unit-tested in `snapshot.rs`
(`reconciliation_loop_pages_to_watermark_and_dedups`) and exposed as the
`Change` / `Cursor` / `caught_up` / `next_cursor` helpers.

---

## 5. Verify two nodes converge

Prereqs: two catalysts (A, B) both built `--features nats`, both pointed at the
same broker, each listing the other in its peer-list TOML, both with
`FED_GOSSIP=nats` and distinct `FED_PEER_ID`s.

1. **Broker reachable.** From each host:
   `nats --server "$FED_NATS_URL" stream ls` should list `FED_PLACES` (created
   lazily on first subscribe/publish).

2. **Consumers up.** Each service logs `places gossip consumer started
   (fed.places.actions)` on boot. If you instead see `consumer not started
   (transport reaches no peers...)`, gossip is disabled — recheck `FED_GOSSIP`
   and the `--features nats` build.

3. **Live push.** Perform a signed write on A (e.g. a place vote). Within a
   second B logs the applied envelope and the change is queryable on B. A does
   **not** re-consume its own write (self-echo is filtered by `origin_peer`).

4. **Catch-up.** Stop B, perform several writes on A, restart B. B reconciles
   via snapshot-pull: `GET /federation/places/changes?since=<B's cursor>` on A
   returns the gap; after applying, B's snapshot watermark matches A's.

5. **Convergence check.** `GET /federation/places/snapshot` on both nodes should
   report equal `latest_*_seq` once quiescent. Re-running any pull applies zero
   new rows (dedup on `signature_hash`).

### Test the transport without a broker

The publish -> consume -> re-verify -> apply -> dedup loop is covered in-process
(no broker) by `tests/gossip_loop.rs`. To exercise the **real** NATS transport,
point the env-gated test at a live broker:

```bash
FED_NATS_URL=nats://127.0.0.1:4222 \
  dcl-shell -c "cargo test -p catalyrst-fed --features nats --test nats_live"
```

With `FED_NATS_URL` unset the test skips, so CI stays broker-free.
