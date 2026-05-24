# catalyrst

A from-scratch Rust implementation of the [Decentraland](https://decentraland.org)
**catalyst content server** — the content-addressed storage and distribution
layer that serves scenes, wearables, emotes, and profiles for the Decentraland
metaverse.

catalyrst aims for behavioral parity with the reference (TypeScript) catalyst
content server: same HTTP API, same content hashing (IPFS CIDs / ADR-45), same
auth-chain verification (including EIP-1654 smart-wallet signatures), the same
overwrite/active-pointer semantics, and the same snapshot/sync protocol.

## Workspace layout

| Crate | Responsibility |
|---|---|
| `catalyrst-types` | Shared domain types (entities, deployments, env config) |
| `catalyrst-hashing` | IPFS CIDv0/CIDv1 content hashing (UnixFS) |
| `catalyrst-crypto` | Auth-chain recovery & verification (ECDSA, EIP-1654) |
| `catalyrst-storage` | Content blob storage backends |
| `catalyrst-db` | PostgreSQL repositories (deployments, pointers, snapshots, …) |
| `catalyrst-validator` | Entity / content / third-party (Merkle) validation |
| `catalyrst-sync` | Snapshot + pointer-changes sync from a catalyst pool |
| `catalyrst-deployer` | Deployment pipeline, GC, pointer management |
| `catalyrst-server` | HTTP server, handlers, and the `catalyrst-live` binary |
| `catalyrst-conformance` | Diffing tool for response parity vs. a reference server |
| `catalyrst-oracle-tests` | Test vectors extracted from a real DB + fixtures |
| `catalyrst-bench` | Criterion benchmarks for hot paths |
| `catalyrst-fuzz` | Fuzz / stress harnesses |

## Prerequisites

- Stable Rust toolchain (`rustup install stable`).
- PostgreSQL 18. The binary defaults to Postgres's standard port `5432` and to
  the socket directory `/run/postgresql`; adjust `POSTGRES_HOST` / `POSTGRES_PORT`
  if your distro differs. On Debian/Ubuntu (`postgresql-18`'s `postgres` role is
  peer-auth with no password — create a dedicated role instead):
  ```bash
  sudo apt install postgresql-18
  sudo -u postgres createuser --pwprompt catalyrst   # choose a password
  sudo -u postgres createdb -O catalyrst content
  ```
- (Optional, for write mode) An Ethereum RPC endpoint (HTTPS-only).

## Build

```bash
cargo build --release --bin catalyrst-live
```

The HTTP stack uses `rustls`; `openssl-sys` is still pulled transitively via
the Helios consensus light-client for now, so a system OpenSSL may be needed
during compilation depending on your platform.

## Documentation

| Topic | Path |
|---|---|
| Endpoint inventory | [docs/endpoint-inventory.md](./docs/endpoint-inventory.md) |
| OpenAPI 3.1 spec | [docs/openapi.yaml](./docs/openapi.yaml) |
| OpenAPI coverage map | [docs/openapi-coverage.md](./docs/openapi-coverage.md) |
| Reference-parity quirks | [docs/lamb2-parity.md](./docs/lamb2-parity.md) |
| Write-path validator pipeline | [docs/write-path.md](./docs/write-path.md) |
| Auth-chain + EIP-1654 | [docs/auth-chain.md](./docs/auth-chain.md) |
| Sync pipeline | [docs/sync-pipeline.md](./docs/sync-pipeline.md) |
| Snapshot CID convergency | [docs/snapshot-generation.md](./docs/snapshot-generation.md) |
| Third-party Merkle verification | [docs/third-party-merkle.md](./docs/third-party-merkle.md) |
| Offline conformance workflow | [docs/conformance-offline.md](./docs/conformance-offline.md) |
| Build-system notes | [docs/build-system.md](./docs/build-system.md) |
| Cloudflare origin allowlist | [docs/cloudflare-ips.md](./docs/cloudflare-ips.md) |
| LiveKit key rotation | [docs/livekit-rotation.md](./docs/livekit-rotation.md) |
| NixOS deployment topics | [docs/sandboxing.md](./docs/sandboxing.md), [docs/networking.md](./docs/networking.md), [docs/postgres.md](./docs/postgres.md), [docs/tls-acme.md](./docs/tls-acme.md), [docs/nginx-edge.md](./docs/nginx-edge.md), [docs/observability.md](./docs/observability.md) |

## Run

The `catalyrst-live` binary runs in one of three modes — read-only (default),
sync replica, or write node — all configured via environment variables. From
a clean checkout, after the prerequisites above:

```bash
# 1. Build the release binary
cargo build --release --bin catalyrst-live

# 2. Run read-only against the local `content` database
#    (POSTGRES_PORT defaults to 5432; POSTGRES_HOST defaults to /run/postgresql)
POSTGRES_CONTENT_USER=catalyrst \
POSTGRES_CONTENT_PASSWORD=<the password you set above> \
POSTGRES_HOST=/var/run/postgresql \
  ./target/release/catalyrst-live
```

Adjust `POSTGRES_HOST` to match your distro's socket directory (Debian/Ubuntu
use `/var/run/postgresql`; the Rust default is `/run/postgresql`).

See **[DEPLOYMENT.md](./DEPLOYMENT.md)** for the full environment-variable
reference, the three operating modes, third-party registry indexing, and the
example NixOS deployment configs in [`nixos/`](./nixos).

## License

Licensed under the **GNU Affero General Public License v3.0** (AGPL-3.0).
See [LICENSE](./LICENSE).
