# catalyrst docs

A curated set focused on what the code does not tell you: composition contracts, deployment topology, invariants with non-obvious rationale, operational sharp edges. Route lists and DTO shapes are deliberately not documented here - the code (`routes.rs` / `handlers/` per crate) is their source of truth, plus [`openapi.yaml`](./openapi.yaml) for the content core (consumed by `scripts/schemathesis/`).

Reading order for a newcomer: repo `README.md`, then [architecture.md](./architecture.md) (member contract, port truth, DB ownership, asset-bundle server, deployment styles), then [build-and-test.md](./build-and-test.md) (toolchain gotchas, flake pins, test harnesses).

## Invariants (why the code is the way it is - don't "fix" these blind)

| Doc | Guards |
|---|---|
| [content-sync.md](./content-sync.md) | peer-pool sync trust boundary, resume/retry shape, snapshot CID convergency |
| [auth.md](./auth.md) | auth-chain verification, EIP-1654, fail-closed defaults |
| [third-party-merkle.md](./third-party-merkle.md) | byte-exact Merkle rules for third-party collections |
| [federation.md](./federation.md) | signed-write envelope contract, gossip vs snapshot-pull |
| [crate-boundaries.md](./crate-boundaries.md) | which crates stay separate and why; merges not to propose |

## Deploying and operating

| Doc | Covers |
|---|---|
| Repo `DEPLOYMENT.md` | content-core modes, env reference, third-party indexing, X-Accel |
| [deploy.md](./deploy.md) | bundle runbook, explorer pointing, gateway mode; worked nginx configs in [deploy/](./deploy/) |
| [operations.md](./operations.md) | postgres, networking/UDP, observability, LiveKit, admin console |

## Verification

| Doc | Covers |
|---|---|
| [testing/abgen-similarity.md](./testing/abgen-similarity.md) | asset-bundle similarity gates: method, tiers, thresholds, open gaps |

When code and a doc disagree, the doc is wrong - fix the doc.
