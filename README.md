# catalyrst

A from-scratch Rust implementation of the [Decentraland](https://decentraland.org)
**service plane**. It began as a port of the **catalyst content server** — the
content-addressed storage and distribution layer that serves scenes, wearables,
emotes, and profiles — and has since grown to cover essentially every backend
service a Decentraland explorer talks to: content + lambdas, the
places/events/worlds/map explorer APIs, the social stack (communities,
comms-gatekeeper, notifications, friends/voice RPC), the creator and
marketplace/data planes, scene-state multiplayer, and a federation layer — so a
full DCL realm can be self-hosted from one Rust workspace.

catalyrst aims for behavioral parity with the reference (TypeScript) services:
the same HTTP/WS APIs, the same content hashing (IPFS CIDs / ADR-45), the same
auth-chain and signed-fetch verification (including EIP-1654 smart wallets), and
the same overwrite/active-pointer and snapshot/sync semantics.

## Workspace layout

45 crates. Each service is its own crate and runs standalone; for deployment,
several are composed into **bundle binaries** (one axum port serving several
services) — see [Service bundles](#service-bundles) and
[`docs/deploy/runbook.md`](./docs/deploy/runbook.md).

### Foundation libraries

| Crate | Responsibility |
|---|---|
| `catalyrst-types` | Shared catalyst domain types (entities, deployments, env config) |
| `catalyrst-hashing` | Content addressing — CIDv0 (`Qm…`) / CIDv1 (`bafy…`) hashing |
| `catalyrst-crypto` | `@dcl/crypto` auth-chain recovery & verification (ECDSA, EIP-1654) |
| `catalyrst-fed` | Federation primitives — EIP-712 envelopes, session delegation, peer registry, rate limits, gossip/snapshot transport |
| `catalyrst-storage` | Content blob storage backends |
| `catalyrst-db` | PostgreSQL repositories (deployments, pointers, snapshots, …) |
| `catalyrst-validator` | Entity / content / third-party (Merkle) validation |
| `catalyrst-sync` | Snapshot + pointer-changes sync from a catalyst pool |
| `catalyrst-deployer` | Deployment pipeline, GC, pointer management |

### Content / catalyst core

| Crate | Ports / serves |
|---|---|
| `catalyrst-server` | The catalyst content-server HTTP layer (`/content`, `/lambdas`, `/about`); ships the `catalyrst-live` binary |

### Service crates (each a standalone binary)

| Crate | Reference service it ports |
|---|---|
| `catalyrst-places` | `places.decentraland.org` REST (federation-aware) |
| `catalyrst-events` | `events.decentraland.org` REST (HTTP-snapshot federation) |
| `catalyrst-archipelago` | `archipelago-workers` — peer clustering, ws-connector, stats |
| `catalyrst-worlds` | `worlds-content-server` — World realm `/about`, permissions, comms |
| `catalyrst-map` | `atlas-server` — map tiles + `map.png` renderer (squid DB) |
| `catalyrst-lists` | `dcl-lists` — curated POI list + ENS banned-name denylist |
| `catalyrst-builder` | `builder-server` (`builder-api`) — collection items, newsletter |
| `catalyrst-camera-reel` | `camera-reel-service` — content-addressed photo store |
| `catalyrst-ab-registry` | `asset-bundle-registry` — AB build status/versions/bundles |
| `catalyrst-communities` | `social-service-ea` community routes (authority-chain federation) |
| `catalyrst-comms` | `comms-gatekeeper` — LiveKit tokens, scene bans, voice, Cast 2.0 |
| `catalyrst-notifications` | `notifications` REST (signed-fetch reader/marker) |
| `catalyrst-badges` | `badges` REST — profile badge state |
| `catalyrst-media` | `autotranslate-server` — LibreTranslate-compatible `/translate` |
| `catalyrst-social-rpc` | `social-service-ea` — dcl-rpc WebSocket (friends, blocks, mutes, voice) |
| `catalyrst-market` | `marketplace-server` REST (squid `marketplace` schema) |
| `catalyrst-economy` | `transactions-server` — meta-transaction relay |
| `catalyrst-price` | CoinGecko price-feed proxy (`/api/v3/simple/price`) over the mana_price archive |
| `catalyrst-credits` | `credits.decentraland.org` — Marketplace Credits program API |
| `catalyrst-rpc` | `rpc.decentraland.org` — method-filtered read-only EVM JSON-RPC relay (HTTP+WS) |
| `catalyrst-explorer-api` | bundles `realm-provider`, `auth-api`, `blocklist`, `builder-api`, `feature-flags` |
| `catalyrst-signatures` | `signatures-server` — LAND/Estate rental-listing signature store |
| `catalyrst-world-storage` | `world-storage-service` — signed-fetch (ADR-44) KV + encrypted env storage |
| `catalyrst-scene-state` | `scene-state-server` — authoritative SDK7 multiplayer scene state (HTTP+WS CRDT) |
| `catalyrst-profile-images` | `profile-images` — avatar thumbnails (local headless-godot render + disk cache) |
| `catalyrst-ab-cdn` | `ab-cdn` — content-addressed static server over abgen's output root |
| `catalyrst-telemetry` | Local Sentry-envelope + Segment sinks — store client telemetry in postgres |

### Service bundles

Deployment aggregates — each composes several service crates onto a single axum
port (members still build/run standalone). An edge proxy terminates TLS and
path-routes; see [`docs/deploy/runbook.md`](./docs/deploy/runbook.md).

| Bundle binary | Port | Members |
|---|---|---|
| `catalyrst-live` (`catalyrst-server`) | 5141 | content, lambdas, about |
| `catalyrst-explore` | 5143 | places, events, archipelago, worlds, map, lists |
| `catalyrst-create` | 5144 | builder, camera-reel, ab-registry |
| `catalyrst-social` | 5145 | communities, comms, notifications, badges, media |
| `catalyrst-data` | 5146 | market, economy, price, credits, rpc |
| `catalyrst-explorer-api` | 5137 | realm-provider, auth-api, blocklist, builder-api, feature-flags |
| `catalyrst-ab-cdn` | 5147 | asset-bundle CDN (LOD / manifest / binaries) |
| `catalyrst-social-rpc` | 5148 | dcl-rpc WebSocket (friends / voice) |
| `catalyrst-scene-state` | 5153 | SDK7 scene-state HTTP + WebSocket |
| `catalyrst-profile-images` | 5154 | avatar thumbnail render/serve |
| `catalyrst-market` | 5133 | standalone marketplace (parallel to `data`'s `/v1`) |

> Downstream packagers may instead run individual service crates as standalone
> units (e.g. `comms`, `places`, `events`, `communities`, `archipelago` each on
> their own port) rather than the composed bundles above.

### Tooling & tests

| Crate | Responsibility |
|---|---|
| `catalyrst-conformance` | Side-by-side parity/diff tester for two catalyst hosts |
| `catalyrst-oracle-tests` | Oracle tests — real vectors extracted from a live DB |
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
cargo build --release --bin catalyrst-live          # content core
# any service crate or bundle is a named binary, e.g.:
cargo build --release --bin catalyrst-social        # social bundle (5145)
cargo build --release --bin catalyrst-explore       # explorer bundle (5143)
cargo build --release --bin catalyrst-market        # standalone marketplace (5133)
```

Binaries land in `target/release/catalyrst-<name>`. See
[`docs/deploy/runbook.md`](./docs/deploy/runbook.md) for the bundle→port map and
the per-service env files.

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
| Gateway mode (stock explorer's `{host}/{subdomain}/{path}` contract) | [docs/deploy/gateway.md](./docs/deploy/gateway.md) |

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
