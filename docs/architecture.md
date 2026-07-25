# Architecture - how the workspace fits together

48 crates. The sync module lives inside `catalyrst-server` (run by `catalyrst-live`; [content-sync.md](./content-sync.md)). `catalyrst-social-service` serves both communities and social-rpc routes (its binaries keep both names). `catalyrst-places` serves POI/banned-names via `lists_router()`. `catalyrst-pulse` owns the transport module.

## 1. The member-crate contract

Every service crate exposes two things (any bundle `main.rs`, e.g. `crates/catalyrst-explore/src/main.rs`):

- `api_router() -> axum::Router<AppState>` - API routes only; no `/ping`/`/status`/`/health`/`/ready`/`/metrics` (duplicate health paths panic when merged);
- `build_state(cfg) -> anyhow::Result<AppState>` - pools, caches, background tasks, the crate's sqlx migrations, same as standalone.

Bundles merge member routers with no path prefix - members serve upstream paths at the bundle root; two upstreams in one bundle (places + events; social-api + comms-gatekeeper) are distinguishable only by the edge's front-host prefix ([deploy.md](./deploy.md)). Bundles fail soft: a member whose `build_state()` errors is dropped, `/health` names it. Each member reads its own env (`Config::from_env`); a bundle env file = the union of its members'.

## 2. Ports: what's real, what's a dev default

Bundles bind `BUNDLE_HTTP_PORT` (host hardcoded loopback); `catalyrst-live` auto-loads `/etc/catalyrst/content.env` at boot (surprises ad-hoc runs); other standalone binaries bind `HTTP_SERVER_HOST`/`HTTP_SERVER_PORT`. Standalone defaults collide; real deployments assign every port via env behind a path-routing edge.

| Default | Claimants |
|---|---|
| 5141 | live |
| 5143/5144/5145/5146 | bundles explore/create/social/data |
| 5145 | builder (collides with social bundle) |
| 5146 | worlds (collides with data bundle) |
| 5147 | abgen, badges |
| 5148 | social-rpc, notifications |
| 5150 | telemetry, credits |
| 5151 | governance, signatures, world-storage |
| 5152 | presence, map, profile-images |
| 5153 | scene-state, rpc |
| 5155 | economy, quests (binds `0.0.0.0` via `QUESTS_BIND`) |
| 8080 | communities REST |

Services with loopback-only admin/money endpoints (market, credits, economy) refuse a non-loopback bind unless their admin bearer token is set (`guard_admin_exposure`) - deliberate.

## 3. Database ownership map

A crate "owns" a DB when its `build_state()` runs `sqlx::migrate!` against it; everything else is a read pool. The required env vars below block boot.

| Owner | DB (required env) | Notable read pools |
|---|---|---|
| server/live | `content` (`POSTGRES_CONTENT_*`; user+password required) | squid `marketplace_squid` (`SQUID_DB_*`, degrades if down) |
| market | dapps DB `marketplace` schema (`DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING` + required `DAPPS_READ_...`, `FAVORITES_...`) | `squid_marketplace` schema (indexer); optional content, usage-grants overlay |
| economy | same dapps DB / `marketplace` schema | `squid_marketplace` |
| credits | `credits` (`CREDITS_PG_CONNECTION_STRING`, 12 migrations) | optional usage-grants + presence pools |
| comms | `comms` (`COMMS_PG_CONNECTION_STRING`) | optional squid (name enrichment), optional places |
| social-service: communities REST | `communities` (`COMMUNITIES_PG_CONNECTION_STRING`) | optional content, mutes |
| social-service: social-rpc | own DB (plain `DATABASE_URL`) | optional content |
| places (incl. lists routes) | places DB (`PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING`; optional separate writer pool) | optional squid |
| events | `places_events` | - |
| worlds | worlds DB | optional squid (NAME-ownership publish authz - **fail-closed deny** when missing) |
| notifications, badges, media, builder, camera-reel, price, signatures, world-storage, telemetry, governance, presence | one DB each (`<SVC>_PG_CONNECTION_STRING`-style) | governance optionally reads external archive DBs (`SNAPSHOT_DATABASE_URL`, `DISCOURSE_DATABASE_URL`) - absence = empty results, never an error |
| map | none owned - reads `squid_marketplace` directly | - |
| explorer-api, scene-state, rpc, profile-images, pulse | **no database** | - |

UNCONFIRMED: `live.rs` never calls `migrate!` on `crates/catalyrst-server/migrations` + `crates/catalyrst-db/migrations` - the deployment applies them out-of-band; same for `catalyrst-places/migrations` and quests' single migration.

## 4. External-world touchpoints

| Dependency | Crates | Env | If absent |
|---|---|---|---|
| Ethereum RPC | server (EIP-1654), economy, world-storage | `ETH_RPC_URL` (server, economy); `RPC_ENDPOINT_ETH` (world-storage); default `rpc.decentraland.org/mainnet` | write path refuses plaintext `http://` at boot; smart-wallet sigs unverifiable |
| Upstream catalyst pool | server sync module | `SYNC_SOURCE` | sync off by default (`SYNC_ENABLED=false`) |
| Registry/blocks subgraphs | validator, server refresher | `THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL`, `BLOCKS_L2_SUBGRAPH_URL` | third-party deploys reject (fail-closed); local index via `THIRD_PARTY_ROOT_SOURCE=squid` removes the dependency |
| LiveKit SFU | comms, worlds, archipelago | `LIVEKIT_HOST`/`_API_KEY`/`_SECRET` | comms/worlds fail fast unless `LIVEKIT_ALLOW_DEV_CREDS=1`; archipelago boots on `devkey`/`devsecret` with a warning, tokens silently invalid against a real SFU ([operations.md](./operations.md)) |
| CoinGecko | price | `COINGECKO_URL`, `PRICE_POLL_ENABLED` (default **false**) | poller off -> stale snapshots -> credits checkout fail-closes on oracle staleness |
| LibreTranslate | media | `TRANSLATE_BACKEND` (default **mock**) | mock answers locally; `http` mode errors without URL |
| Headless Godot | profile-images | `PROFILE_IMAGES_BACKEND`/`_GODOT_BIN` | auto-selects proxy (if origin set) or disabled |
| NATS broker | fed gossip | `FED_GOSSIP=nats` + feature build | no-op publisher; snapshot-pull still converges ([federation.md](./federation.md)) |
| Stripe | credits | `STRIPE_SECRET_KEY`/`_WEBHOOK_SECRET` | card purchase endpoints 501; service boots |
| SendGrid | notifications | `SENDGRID_API_KEY` | email silently disabled (`is_enabled=false`) |

No external IPFS gateway: "IPFS" = CID computation/validation; content bytes live on local disk or peer content servers. `thirdweb` is never a server dependency - the credits PurchaseIntent is signed client-side; the server only recovers the signer.

One deliberate intra-stack HTTP dependency: scene-state's `~system/SignedFetch` is origin-locked to the world-storage URL (`STORAGE_URL`; https-only unless `STORAGE_ALLOW_HTTP=1` for loopback). Writes use authoritative-storage delegations (`DELEGATION_MINTER_URL`/`_TOKEN`; dev: pre-minted `STORAGE_DELEGATION`) - flow in [auth.md](./auth.md).

## 5. Asset bundles - served by upstream abgen

Catalyrst carries no asset-bundle code; converter, ab-cdn JIT server, LOD generator, and AB-registry live upstream in [decentraland/abgen](https://github.com/decentraland/abgen), consumed via the `abgen` flake input re-exposing `packages.abgen` (server) and `packages.abgen-compare` (parity/inspection). One binary serves everything on :5147: corpus bundles, JIT conversion on miss, LODs, ISS descriptors, `/entities/active|versions`, profiles, and - when `CONTENT_PG_CONNECTION_STRING` (URL form) is set - the signed registry surface (`/entities/status`, `/queues/*`, `/denylist*`, `/registry`, `/flush-cache`). Env: `ABGEN_ROOT`, `ABGEN_SHADER_BUNDLE`, `ABGEN_OUT_ROOT`, `ABGEN_CACHE_DIR`, `ABGEN_CATALYST_URL` + LOD-JIT lane vars (deployed set: the deployment's `catalyrst-abgen` env file). Change AB behavior upstream (fork-PRs via `eordano/abgen`; never push upstream), then bump the flake input.

## 6. Deployment styles

1. **NixOS module** - `flake.nix` exports `nixosModules.catalyrst` (`nixos/configuration.nix`): nginx (TLS/rate-limits/X-Accel), Postgres 18 (least-privilege-ownership oneshot), `catalyrst-sync` unit (runs `catalyrst-live`), marketplace-squid Node processors, Prometheus/exporters/alerts, Cloudflare IP refresh, optional comms block (LiveKit/NATS/archipelago-workers/Pulse). `services.catalyrst.enable = true;`.
2. **Template units** - `nixos/systemd/*.service`: eight standalone units (content, sync, the four bundles, social-rpc, abgen), `EnvironmentFile=` placeholders, for non-Nix hosts; not referenced by the NixOS module.
3. **Per-service standalone** - each member crate as its own unit on its own port, ignoring bundles; the reference deployment does this.

Flake facts not visible from `cargo`: per-service packages + a `catalyrst-all` mega-package (~13 binaries); pin/patch details (librusty_v8, uWebSockets ABI swap, `doCheck = false`, `OPENSSL_NO_VENDOR=1`) in [build-and-test.md](./build-and-test.md).

`/about` comms identity strings have a pinned shape (`commsVersion = <node-version>+pulse-<rev>`, `commsCommitHash = <archipelago-rev>+<pulse-rev>`) consumed by catalyst-monitor - don't change separators.

## 7. Repo periphery (non-code)

- **`contracts/LandilerEscrow.sol`** - 15-day reclaimable custody escrow for wearables/emotes on Polygon. Not in the cargo build; consumed by address only (`LANDILER_ESCROW_ADDRESS`, read by credits/economy). Off-chain half: `usage_grants` overlay (`crates/catalyrst-market/migrations/0007_usage_grants.sql`).
- **`secrets/`** - gitignored; secrets ride env vars (`EnvironmentFile=`/`LoadCredential=`); no dotenv loader except `catalyrst-live`'s `/etc/catalyrst/content.env`.
- **`data/`** - `catalyrst-worlds` runtime content store (`WORLDS_CONTENT_DIR` default `./data/worlds/contents`): CID-addressed blobs + auth files for locally deployed Worlds, not test fixtures.
- **`seed-third-party.sql`** - manual seed of ~27 third-party Merkle roots into `squid_marketplace.third_party`; alternative to the built-in Rust refresher (DEPLOYMENT.md section 2) - pick one writer.
- **`scripts/schemathesis/`** - property-based API fuzzing against [`docs/openapi.yaml`](./openapi.yaml); checks listed in [build-and-test.md](./build-and-test.md).
- **`nixos/landing/index.html`** - the `GET /` landing page.

