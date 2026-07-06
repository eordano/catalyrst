# catalyrst docs

A curated set focused on what the code does not tell you: composition contracts, deployment topology, parity posture, invariants with non-obvious rationale, operational sharp edges. Route lists and DTO shapes are deliberately not documented here - the code (`routes.rs` / `handlers/` per crate) is their source of truth, plus [`openapi.yaml`](./openapi.yaml) for the content core (consumed by `scripts/schemathesis/`).

Reading order for a newcomer: repo `README.md` (what catalyrst is, crate/bundle tables), then [architecture.md](./architecture.md) (member contract, port truth, DB ownership, external touchpoints, deployment styles, repo periphery), then [build-and-test.md](./build-and-test.md) (toolchain gotchas, flake pins, the test harnesses).

## Invariants (why the code is the way it is - don't "fix" these blind)

| Doc | Guards |
|---|---|
| [sync.md](./sync.md) | peer-pool sync: trust boundary, resume semantics, retry shape |
| [snapshots.md](./snapshots.md) | network-wide snapshot CID convergency |
| [auth.md](./auth.md) | auth-chain verification, EIP-1654, fail-closed defaults |
| [third-party-merkle.md](./third-party-merkle.md) | byte-exact Merkle rules for third-party collections |
| [federation.md](./federation.md) | signed-write envelope contract, gossip vs snapshot-pull |
| [asset-bundles.md](./asset-bundles.md) | AB server transparency invariant + the three validation gates |

## Deploying

| Doc | Covers |
|---|---|
| Repo `DEPLOYMENT.md` | content-core modes (read-only / sync / write), env reference, third-party indexing, X-Accel |
| [deploy/runbook.md](./deploy/runbook.md) | full bundle-stack bring-up on one TLS host |
| [deploy/explorer-pointing.md](./deploy/explorer-pointing.md) | full-coverage client rewrite + edge path-routing |
| [deploy/gateway.md](./deploy/gateway.md) | stock explorer's `{gateway}/{subdomain}/{path}` contract, zero client change |
| [deploy/nginx-catalyrst-bundles.conf](./deploy/nginx-catalyrst-bundles.conf), [deploy/nginx-catalyrst-gateway.conf](./deploy/nginx-catalyrst-gateway.conf) | worked edge configs |

## Operating

| Doc | Covers |
|---|---|
| [operations/postgres.md](./operations/postgres.md) | ownership bootstrap, tuning, pgbouncer session-mode rationale |
| [operations/networking.md](./operations/networking.md) | firewall/UDP, CDN-IP refresh, sandbox carve-outs |
| [operations/observability.md](./operations/observability.md) | scrape targets, alerts, known monitoring gaps |
| [operations/livekit.md](./operations/livekit.md) | key rotation, dev-key trap, media-vs-signaling failure modes |
| [operations/admin-console.md](./operations/admin-console.md) | operator surface, session auth, default-safe posture |

Trust policy: every page carries a `Status:` banner naming when and how it was verified; claims inherited from the full audit and not re-checked since say so explicitly. When code and a doc disagree, the doc is wrong - fix the doc, and prefer adding provenance over deleting history.

The previous docs tree (108 files: per-service parity/verification/e2e dossiers, endpoint inventory, explorer-pointing dossier incl. `CatalyrstUrlsSource.cs`, abgen compat bag, admin-console design, pulse implementation plan) was removed and remains recoverable:

```bash
git ls-tree -r --name-only ff400cab^ -- catalyrst/docs/
git show ff400cab^:catalyrst/docs/<path>
```
