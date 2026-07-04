# Architecture — how the workspace actually fits together

> Status: written 2026-07-04 against current code (crate configs, bundle
> mains, flake, nixos module). Facts below cite their source files; items the
> code doesn't pin down are marked UNCONFIRMED.

The README covers *what* catalyrst is and lists the crates. This page covers
what the crate list doesn't tell you: the composition contract, who owns which
database, which defaults are real and which are dev conveniences, what the
process talks to on the outside, and where the repo's non-code pieces plug in.

## 1. The member-crate contract

Every service crate exposes two things for composition
(see any bundle `main.rs`, e.g. `crates/catalyrst-explore/src/main.rs`):

- `api_router() -> axum::Router<AppState>` — real API routes only, **no**
  `/ping`/`/status`/`/health`/`/ready`/`/metrics` (health routes would panic
  as duplicate paths when merged);
- `build_state(cfg) -> anyhow::Result<AppState>` — pools, caches, background
  tasks, and the crate's **sqlx migrations**, exactly as the standalone binary
  does.

A bundle merges member routers **with no per-member path prefix** — every
member serves its real upstream paths at the bundle root. Consequences:

- Two upstreams in one bundle (places + events; social-api + comms-gatekeeper)
  are only distinguishable by the edge's front-host prefix
  (see [deploy/explorer-pointing.md](./deploy/explorer-pointing.md)).
- Bundles **fail soft**: a member whose `build_state()` errors is dropped and
  the bundle serves its healthy siblings; `/health` names the down member.
- Each member reads its own env (`Config::from_env`) — a bundle env file is
  the union of its members' env files.

## 2. Ports: what's real, what's a dev default

Only two kinds of port configuration exist:

- Bundles bind `BUNDLE_HTTP_PORT` (host hardcoded loopback): explore 5143,
  create 5144, social 5145, data 5146.
- `catalyrst-live` binds `CATALYRST_PORT` (default 5141) — **not**
  `HTTP_SERVER_PORT` — and additionally auto-loads `/etc/catalyrst/content.env`
  at boot (`live.rs`), which surprises people running it ad hoc on a host that
  has that file.
- Every other standalone crate binds `HTTP_SERVER_HOST`/`HTTP_SERVER_PORT`.

**The standalone defaults collide and are not a deployment plan.** Literal
defaults today: 5151 is claimed by governance, signatures, world-storage AND
lists; 5152 by presence, map AND profile-images; 5150 by telemetry and
credits; 5148 by social-rpc and notifications; 5147 by abgen and badges; 5153
by scene-state and rpc; 5155 by economy and quests (quests binds `0.0.0.0`,
via `QUESTS_BIND`); communities defaults to 8080. Worlds' standalone default
(5146) collides with the data bundle, builder's (5145) with the social bundle.
Real deployments assign every port via env and front them with a
path-routing edge; treat any port table in prose as that deployment's
convention, not code truth.

Services with loopback-only admin/money endpoints (market, credits, economy)
**refuse to boot** on a non-loopback bind unless their admin bearer token is
set (`guard_admin_exposure` in their configs) — deliberate.

## 3. Database ownership map

A crate "owns" a DB when its `build_state()` runs `sqlx::migrate!` against it;
everything else is a read pool. Required-to-boot connection strings are the
boot-blocking env vars.

| Owner crate | DB (required env) | Notable read pools |
|---|---|---|
| server/live | `content` (`POSTGRES_CONTENT_*`; user+password required) | squid `marketplace_squid` (`SQUID_DB_*`, degrades if down) |
| market | `marketplace` schema in the dapps DB (`DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING`, + required `DAPPS_READ_…` and `FAVORITES_…`) | `squid_marketplace` schema (indexer), optional content, optional usage-grants overlay |
| economy | same dapps DB / `marketplace` schema | `squid_marketplace` |
| credits | `credits` (`CREDITS_PG_CONNECTION_STRING`, 12 migrations — the deepest schema) | optional usage-grants + presence pools |
| comms | `comms` (`COMMS_PG_CONNECTION_STRING`) | optional squid (name enrichment), optional places |
| communities | `communities` | optional content, mutes |
| places | places DB (`PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING`; optional separate writer pool) | optional squid |
| events | `places_events` | — |
| worlds | worlds DB | optional squid (NAME-ownership publish authz — **fail-closed deny** when missing) |
| notifications, badges, media, builder, camera-reel, price, signatures, world-storage, telemetry, governance, presence, lists, social-rpc, registry | one DB each (`<SVC>_PG_CONNECTION_STRING`-style; social-rpc uses plain `DATABASE_URL`) | governance optionally reads external archive DBs (`SNAPSHOT_DATABASE_URL`, `DISCOURSE_DATABASE_URL`) — absence means honest-empty, never an error |
| map | none owned — reads `squid_marketplace` directly | — |
| explorer-api, scene-state, rpc, profile-images, pulse | **no database** | — |

UNCONFIRMED (flagged, not resolved): the content-DB migrations under
`crates/catalyrst-server/migrations` + `crates/catalyrst-db/migrations` are
not applied by `live.rs` (no `migrate!` call) — they're applied out-of-band by
the deployment; same for `catalyrst-places/migrations` and whether quests
applies its single migration.

## 4. External-world touchpoints

What a full deployment talks to besides its own Postgres — and what happens
when each is absent:

| Dependency | Crates | Env | If absent |
|---|---|---|---|
| Ethereum RPC | server (EIP-1654), economy, world-storage | `ETH_RPC_URL` (default `rpc.decentraland.org/mainnet`) | write path refuses plaintext `http://` at boot; smart-wallet sigs unverifiable without it |
| Upstream catalyst pool | sync | `SYNC_SOURCE` | sync off by default (`SYNC_ENABLED=false`) |
| Registry/blocks subgraphs | validator, server refresher | `THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL`, `BLOCKS_L2_SUBGRAPH_URL` | third-party deploys reject (fail-closed); local index via `THIRD_PARTY_ROOT_SOURCE=squid` removes the dependency |
| LiveKit SFU | comms, worlds, archipelago | `LIVEKIT_HOST`/`_API_KEY`/`_SECRET` | boots on `devkey`/`devsecret` with a warning — tokens are silently invalid against a real SFU (see [operations/livekit.md](./operations/livekit.md)) |
| CoinGecko | price | `COINGECKO_URL`, `PRICE_POLL_ENABLED` (default **false**) | poller off ⇒ snapshots go stale ⇒ credits checkout fail-closes on oracle staleness |
| LibreTranslate | media | `TRANSLATE_BACKEND` (default **mock**) | mock backend answers locally; `http` mode errors if URL missing |
| Headless Godot | profile-images | `PROFILE_IMAGES_BACKEND`/`_GODOT_BIN` | auto-selects proxy (if origin set) or disabled |
| NATS broker | fed gossip | `FED_GOSSIP=nats` + feature build | no-op publisher; snapshot-pull still converges ([federation.md](./federation.md)) |
| Stripe | credits | `STRIPE_SECRET_KEY`/`_WEBHOOK_SECRET` | card purchase endpoints 501; service boots |
| SendGrid | notifications | `SENDGRID_API_KEY` | email sending silently disabled (`is_enabled=false`) |

There is **no external IPFS gateway** anywhere: "IPFS" in this codebase means
CID computation/validation; content bytes live on local disk or come from
peer content servers.

`thirdweb` never appears as a server dependency — the credits PurchaseIntent
is signed client-side; the server only recovers the signer.

## 5. Deployment styles (the repo ships two, plus a third in practice)

1. **NixOS module** — `flake.nix` exports `nixosModules.catalyrst`
   (`nixos/configuration.nix`): nginx with TLS/rate-limits/X-Accel, Postgres
   18 with the least-privilege ownership oneshot, the `catalyrst-sync` unit,
   marketplace-squid Node processors, Prometheus + exporters + alert rules,
   Cloudflare IP refresh, optional comms block (LiveKit + NATS + archipelago
   workers + Pulse). `services.catalyrst.enable = true;` and most of the
   operations docs here are descriptions of what this module wires.
2. **Template units** — `nixos/systemd/*.service`: eight standalone units
   (content, sync, the four bundles, social-rpc, abgen) with
   `EnvironmentFile=` placeholders, for non-Nix hosts. These are *not*
   referenced by the NixOS module; they're a parallel style.
3. **Per-service standalone** — downstream packagers can ignore bundles
   entirely and run each member crate as its own unit on its own port. The
   reference deployment does exactly this (that's why its port map doesn't
   match the bundle defaults).

Flake facts that aren't visible from `cargo`: per-service packages plus a
`catalyrst-all` mega-package (~13 binaries), `librusty_v8` pinned via
`crates/catalyrst-scene-state/nix/librusty_v8.nix`, `doCheck = false`
everywhere, `OPENSSL_NO_VENDOR=1`, and an `archipelago-workers` npm build that
swaps `uWebSockets.js` for a Node-24-ABI version and carries a vendored
`package-lock.json` (upstream only ships `yarn.lock`). The `/about`
comms identity strings have a pinned shape
(`commsVersion = <node-version>+pulse-<rev>`,
`commsCommitHash = <archipelago-rev>+<pulse-rev>`) consumed by
catalyst-monitor — don't change separators.

## 6. Repo periphery (non-code)

- **`contracts/LandilerEscrow.sol`** — 15-day reclaimable custody escrow for
  wearables/emotes on Polygon. **Not part of the cargo build**; consumed by
  address only (`LANDILER_ESCROW_ADDRESS`, read by credits + economy). The
  off-chain half is the `usage_grants` overlay
  (`crates/catalyrst-market/migrations/0007_usage_grants.sql`).
- **`secrets/`** — gitignored; runtime secrets ride env vars set by the
  deployment (`EnvironmentFile=`/`LoadCredential=`). No dotenv loader in the
  crates (the one exception: `catalyrst-live`'s `/etc/catalyrst/content.env`).
- **`data/`** — runtime content store for `catalyrst-worlds`
  (`WORLDS_CONTENT_DIR` default `./data/worlds/contents`), holding
  CID-addressed blobs + auth files for locally deployed Worlds. Not test
  fixtures.
- **`seed-third-party.sql`** — manual seed of ~27 third-party Merkle roots
  into `squid_marketplace.third_party`; alternative to the built-in Rust
  refresher (DEPLOYMENT.md §2). Pick one writer.
- **`scripts/schemathesis/`** — property-based API fuzzing of a running
  server against [`docs/openapi.yaml`](./openapi.yaml) (4 custom checks:
  5xx, schema conformance, CORS headers, error-body shape).
- **`nixos/landing/index.html`** — the `GET /` landing page.

## 7. Where the per-service deep dives went

The 2026-07-03 audit tree — per-service `parity/`, `verification/`,
`e2e-test/` dossiers (69 files), the endpoint inventory, the explorer-pointing
dossier with `CatalyrstUrlsSource.cs`, and the abgen compat bag — was removed
from the working tree in the docs cleanup but is fully recoverable:

```bash
git show ff400cab^:catalyrst/docs/<path>     # single file
git ls-tree -r --name-only ff400cab^ -- catalyrst/docs/
```

[status-and-parity.md](./status-and-parity.md) carries forward the durable
conclusions.
