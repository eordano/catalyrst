# catalyrst

From-scratch Rust implementation of the [Decentraland](https://decentraland.org)
service plane - every backend service an explorer talks to - so a full DCL
realm can be self-hosted from one Rust workspace. Behavioral parity with the
reference TypeScript services: same HTTP/WS APIs, content hashing (IPFS CIDs /
ADR-45), auth-chain and signed-fetch verification (including EIP-1654 smart
wallets), and overwrite/active-pointer + snapshot/sync semantics.

## Workspace layout

54 crates. Each service crate runs standalone; deployments compose several
into bundle binaries (one axum port) - see [Service bundles](#service-bundles).

### Foundation libraries

| Crate | Responsibility |
|---|---|
| `catalyrst-types` | Shared catalyst domain types (entities, deployments, env config) |
| `catalyrst-envcfg` | Env-var parsing helpers + the standard `--help`/env-docs CLI contract shared by every service binary |
| `catalyrst-hashing` | Content addressing - CIDv0 (`Qm...`) / CIDv1 (`bafy...`) hashing |
| `catalyrst-crypto` | `@dcl/crypto` auth-chain recovery & verification (ECDSA, EIP-1654) |
| `catalyrst-fed` | Federation primitives - EIP-712 envelopes, session delegation, peer registry, rate limits, gossip/snapshot transport |
| `catalyrst-storage` | Content blob storage backends |
| `catalyrst-db` | PostgreSQL repositories (deployments, pointers, snapshots, ...) |
| `catalyrst-validator` | Entity / content / third-party (Merkle) validation |
| `catalyrst-sync` | Snapshot + pointer-changes sync from a catalyst pool |
| `catalyrst-deployer` | Deployment pipeline, GC, pointer management |
| `catalyrst-registry` | Asset-bundle-registry surface (active-entity + AB status/versions/bundles + profiles/worlds/admin handlers) - library only; the standalone `catalyrst-ab-registry` binary is retired, now consumed by `catalyrst-create` and `catalyrst-abgen` |
| `catalyrst-enet` | ENet reliable-UDP protocol (connect, channels, reliable/unreliable/sequenced delivery, acks, fragmentation) - foundation for `catalyrst-pulse` |

### Content / catalyst core

| Crate | Ports / serves |
|---|---|
| `catalyrst-server` | The catalyst content-server HTTP layer (`/content`, `/lambdas`, `/about`); ships the `catalyrst-live` binary |

### Service crates (each a standalone binary)

| Crate | Reference service it ports |
|---|---|
| `catalyrst-places` | `places.decentraland.org` REST (federation-aware) |
| `catalyrst-events` | `events.decentraland.org` REST (HTTP-snapshot federation) |
| `catalyrst-archipelago` | `archipelago-workers` - peer clustering, ws-connector, stats |
| `catalyrst-worlds` | `worlds-content-server` - World realm `/about`, permissions, comms |
| `catalyrst-map` | `atlas-server` - map tiles + `map.png` renderer (squid DB) |
| `catalyrst-lists` | `dcl-lists` - curated POI list + ENS banned-name denylist |
| `catalyrst-builder` | `builder-server` (`builder-api`) - collection items, newsletter |
| `catalyrst-camera-reel` | `camera-reel-service` - content-addressed photo store |
| `catalyrst-communities` | `social-service-ea` community routes (authority-chain federation) |
| `catalyrst-comms` | `comms-gatekeeper` - LiveKit tokens, scene bans, voice, Cast 2.0 |
| `catalyrst-notifications` | `notifications` REST (signed-fetch reader/marker) |
| `catalyrst-badges` | `badges` REST - profile badge state |
| `catalyrst-media` | `autotranslate-server` - LibreTranslate-compatible `/translate` |
| `catalyrst-social-rpc` | `social-service-ea` - dcl-rpc WebSocket (friends, blocks, mutes, voice) |
| `catalyrst-pulse` | `Pulse` - real-time social/MMO server over ENet (sessions, rooms, presence, message routing) |
| `catalyrst-quests` | `quests` - quests/achievements backend (REST + dcl-rpc `QuestsService` over signed-auth-chain WebSocket) |
| `catalyrst-market` | `marketplace-server` REST (squid `marketplace` schema) |
| `catalyrst-economy` | `transactions-server` - meta-transaction relay |
| `catalyrst-price` | CoinGecko price-feed proxy (`/api/v3/simple/price`) over the mana_price archive; optional built-in CoinGecko poller (`PRICE_POLL_ENABLED`) writes its own snapshots |
| `catalyrst-credits` | `credits.decentraland.org` - Marketplace Credits program API |
| `catalyrst-rpc` | `rpc.decentraland.org` - method-filtered read-only EVM JSON-RPC relay (HTTP+WS) |
| `catalyrst-explorer-api` | bundles `realm-provider`, `auth-api`, `blocklist`, `builder-api`, `feature-flags` |
| `catalyrst-signatures` | `signatures-server` - LAND/Estate rental-listing signature store |
| `catalyrst-world-storage` | `world-storage-service` - signed-fetch (ADR-44) KV + encrypted env storage |
| `catalyrst-scene-state` | `scene-state-server` - authoritative SDK7 multiplayer scene state (HTTP+WS CRDT) |
| `catalyrst-profile-images` | `profile-images` - avatar thumbnails (local headless-godot render + disk cache) |
| `catalyrst-abgen` | `ab-cdn` - content-addressed AB server over abgen's output root + in-process converter |
| `catalyrst-telemetry` | Local Sentry-envelope + Segment sinks - store client telemetry in postgres |
| `catalyrst-governance` | `governance.decentraland.org/api` - proposals/projects/budgets/vestings/members archiver + paged read API (one-shot `backfill`, incremental `sync`, optional background poll) |
| `catalyrst-presence` | Unified user-count history - peers/islands/hot-scenes + per-scene/per-world occupancy snapshots & history; collects from the local archipelago/comms services (replaces the umbrella archipelago/comms/worlds-membership archivers) |
| `catalyrst-deploy-signer` | `sdk-commands deploy`'s linker dApp - one-shot local web page that collects a wallet signature and deploys a scene/World to a content server. Second mode: `--serve-delegations` runs a long-lived authoritative-storage delegation minter (`POST /delegations`; `--delegation-ttl-secs`, default 3600). An empty/absent `--delegation-token` forces a loopback-only bind; a non-empty token enables `--bind` and gates minting behind a timing-safe bearer check |
| `catalyrst-preview-tunnel` | HTTP tunnel for scene previews - allocates public URLs and relays requests over a bearer-authenticated trunk WebSocket |

### Service bundles

Members still build/run standalone. An edge proxy terminates TLS and
path-routes; see [`docs/deploy/runbook.md`](./docs/deploy/runbook.md).

| Bundle binary | Port | Members |
|---|---|---|
| `catalyrst-live` (`catalyrst-server`) | 5141 | content, lambdas, about |
| `catalyrst-explore` | 5143 | places, events, archipelago, worlds, map, lists |
| `catalyrst-create` | 5144 | builder, camera-reel, registry |
| `catalyrst-social` | 5145 | communities, comms, notifications, badges, media |
| `catalyrst-data` | 5146 | market, economy, price, credits, rpc |
| `catalyrst-explorer-api` | 5137 | realm-provider, auth-api, blocklist, builder-api, feature-flags |
| `catalyrst-abgen` | 5147 | asset-bundle CDN (LOD / manifest / binaries) |
| `catalyrst-social-rpc` | 5148 | dcl-rpc WebSocket (friends / voice) |
| `catalyrst-governance` | 5151 | governance archive + read API (standalone) |
| `catalyrst-presence` | 5152 | user-count history collector + read API (standalone) |
| `catalyrst-scene-state` | 5153 | SDK7 scene-state HTTP + WebSocket |
| `catalyrst-profile-images` | 5154 | avatar thumbnail render/serve |
| `catalyrst-market` | 5133 | standalone marketplace (parallel to `data`'s `/v1`) |

> Downstream packagers may instead run individual service crates standalone,
> each on its own port.

### Tooling & tests

| Crate | Responsibility |
|---|---|
| `dcl-one-sdk` | Binary-compatible Rust replacement for `@dcl/sdk-commands` - compile, preview-serve, and deploy SDK7 scenes |
| `catalyrst-conformance` | Side-by-side parity/diff tester for two catalyst hosts |
| `catalyrst-oracle-tests` | Oracle tests - real vectors extracted from a live DB |
| `catalyrst-bench` | Criterion benchmarks for hot paths |
| `catalyrst-fuzz` | Fuzz / stress harnesses |

## Prerequisites

- Stable Rust toolchain (`rustup install stable`).
- PostgreSQL 18. Defaults: port `5432`, socket directory `/run/postgresql`;
  adjust `POSTGRES_HOST` / `POSTGRES_PORT` if your distro differs. On
  Debian/Ubuntu the `postgres` role is peer-auth with no password - create a
  dedicated role:
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

On Nix/NixOS, `nix develop` provides the full toolchain; `nix build
.#catalyrst` / `.#catalyrst-all` build pinned artifacts. Binaries land in
`target/release/catalyrst-<name>`; see
[`docs/deploy/runbook.md`](./docs/deploy/runbook.md) for the bundle->port map
and per-service env files. The HTTP stack uses `rustls`, but `openssl-sys` is
pulled transitively via the Helios consensus light-client, so a system OpenSSL
may be needed during compilation.

## Documentation

Start at [docs/README.md](./docs/README.md) (index + reading order + trust policy).

| Topic | Path |
|---|---|
| Architecture - composition contract, port truth, DB ownership, external deps | [docs/architecture.md](./docs/architecture.md) |
| Building & testing (incl. NixOS notes, flake pins, test harnesses) | [docs/build-and-test.md](./docs/build-and-test.md) |
| Sync pipeline invariants | [docs/sync.md](./docs/sync.md) |
| Snapshot CID convergency | [docs/snapshots.md](./docs/snapshots.md) |
| Auth-chain + EIP-1654 | [docs/auth.md](./docs/auth.md) |
| Third-party Merkle verification | [docs/third-party-merkle.md](./docs/third-party-merkle.md) |
| Federation (signed writes, gossip, snapshot-pull) | [docs/federation.md](./docs/federation.md) |
| Asset-bundle server + validation gates | [docs/asset-bundles.md](./docs/asset-bundles.md) |
| OpenAPI 3.1 spec (content core) | [docs/openapi.yaml](./docs/openapi.yaml) |
| Deploy: bundle runbook, explorer pointing, gateway mode, nginx configs | [docs/deploy/](./docs/deploy/) |
| Operations: postgres, networking, observability, LiveKit, admin console | [docs/operations/](./docs/operations/) |

## Run

`catalyrst-live` runs in one of three modes - read-only (default), sync
replica, or write node - all configured via environment variables:

```bash
cargo build --release --bin catalyrst-live
# read-only against the local `content` database
POSTGRES_CONTENT_USER=catalyrst \
POSTGRES_CONTENT_PASSWORD=<the password you set above> \
POSTGRES_HOST=/var/run/postgresql \
  ./target/release/catalyrst-live
```

Set `POSTGRES_HOST` to your distro's socket directory (Debian/Ubuntu
`/var/run/postgresql`; Rust default `/run/postgresql`). See
[DEPLOYMENT.md](./DEPLOYMENT.md) for the full environment-variable reference,
the three operating modes, third-party registry indexing, and the NixOS
deployment configs in [`nixos/`](./nixos).

## License

GNU Affero General Public License v3.0 (AGPL-3.0). See [LICENSE](./LICENSE).

## Related repositories

The standalone asset-bundle converter + AB-parity compare pipeline
(`crates/catalyrst-abgen` plus its tooling) is also available as
[abgen-rs](https://github.com/eordano/abgen-rs).
