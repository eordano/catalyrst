# Service scope: core vs overlay vs instance-infra

Status: canonical as of 2026-07-31. Enforced by `umbrella/scripts/check-ports.py`
(every `deploy/ports.nix` `umbrella` entry must carry a valid `scope`).

Guardrail for absorbing umbrella into catalyrst and going public: pins down,
per service, whether it belongs to the upstreamable realm product or to this
operator's private extras.

- core: the upstreamable realm product. A stranger standing up their own
  catalyrst realm gets these. May depend only on other core services and
  instance-infra; never on overlay or lore (`:5433`) at runtime.
- overlay: this operator's extras (analytics, previews, benches, experiments).
  May drink from core; core never drinks from overlay.
- instance-infra: per-instance plumbing (front site, reverse proxy, database,
  message bus). Deployment-specific; the public product documents the
  requirement, the instance owns the incarnation.

## Classification

| Service | Scope | Rationale |
|---|---|---|
| content | core | Catalyst content server — the heart of the realm |
| sync | core | Entity sync from peer catalysts into the content DB |
| market | core | Marketplace API (port of `marketplace-server`) over the squid DB |
| places | core | Port of `places.decentraland.org` REST API |
| events | core | Port of `events.decentraland.org` REST API |
| communities | core | Port of `social-service-ea` community routes |
| explorer-api | core | Client-facing explorer API surface |
| comms | core | Realm comms — clients cannot connect without it |
| archipelago | core | Peer clustering/island assignment for comms |
| worlds | core | Worlds hosting + NAME-gated publish |
| world-storage | core | Storage backend for worlds |
| builder | core | Builder API (`/v1/collections`, storage, newsletter) |
| badges | core | Badges service, part of the platform surface |
| credits | core | Credits/escrow loop, client-facing economy surface |
| abgen | core | Asset-bundle CDN + registry + converter; upstream equivalent is `asset-bundle-converter`/ab-cdn — clients need it |
| notifications | core | Notifications API, client-facing |
| social-rpc | core | Friends/social RPC over websocket |
| telemetry | core | Client telemetry ingest endpoint (sites `TELEMETRY_URL` contract) — see judgment calls |
| governance | core | Governance API — its former lore read is a closed exception, see below |
| presence | core | Who's-online/user-count, realm social surface |
| economy | core | Economy service, client-facing |
| price | core | Pricing service, client-facing |
| media | core | Media service, client-facing |
| map | core | Map/atlas tiles, client-facing |
| camera-reel | core | Photo upload/reel, client-facing signed API |
| scene-state | core | Authoritative SDK7 scene state |
| squid-eth-metrics | core | Live chain ingest (eth processor metrics) — the realm's on-chain view |
| squid-polygon-metrics | core | Live chain ingest (polygon processor metrics) |
| squid-api | core | RETIRED 2026-07-31 (port reserved); was the GraphQL read layer of the core chain-ingest family |
| metabase | overlay | Operator analytics on top of the stack; no client depends on it |
| slides | overlay | Operator's slide decks |
| preview-tunnel | overlay | Dev preview tunnel |
| abgen-compare | overlay | QA/comparison harness for abgen output, not a product surface |
| vrm-renders | overlay | VRM avatar render experiments (port reserved, no unit) |
| editor-scene | overlay | Creator Hub editor system-scene preview (dev loop, builds+watches) |
| project-realm | overlay | Creator Hub empty-project realm preview (dev loop) |
| bvwebgpu | overlay | Scene-pack supply experiment (pinned `abgen-bvwebgpu-*` binary from `data/bin`, no crate in tree) — see judgment calls |
| bvimposters | core | Imposter supply for the bevy-explorer web client; realm-independent, mirrors upstream bevy-explorer's community imposter CDN — see judgment calls |
| sites | instance-infra | This instance's front site (play/market SPA host); another operator ships their own front |
| nginx-http / nginx-tls | instance-infra | Reverse proxy of this instance |
| postgres | instance-infra | This instance's `:5434` cluster; the product requires "a postgres", not this one |
| nats-client / nats-monitor | instance-infra | Message bus of this instance; core services declare the dependency, the instance provides the bus |
| livekit-signaling / livekit-media-udp | instance-infra | LiveKit SFU of this instance (`umbrella-livekit.service`); comms/worlds/archipelago (core) mint tokens against "a LiveKit", the instance provides this one |

## Judgment calls (deviations and hesitations, recorded)

- telemetry -> core (starting judgment had a `?`): the ingest endpoint is part
  of the client contract (sites points `TELEMETRY_URL` at it; sentry/segment
  sinks). Downstream of ingestion (metabase dashboards) is overlay. If the
  sink config ever hardwires operator-private destinations, split the
  endpoint (core) from the sink config (instance concern).
- bvimposters -> core (starting judgment leaned overlay): it is a real crate
  (`catalyrst-bvimposters`), crc-keyed and explicitly realm-independent, with
  community-CDN read-through and local bake-on-miss — the shipped bevy-web client
  consumes it and upstream bevy-explorer has the same facility. Engine-specific,
  but the bevy-web client is part of this product.
- bvwebgpu -> overlay (starting judgment leaned overlay; confirmed): unlike
  bvimposters it has no crate in the tree — the unit runs a pinned experimental
  binary (`data/bin/abgen-bvwebgpu-<hash>`). Promote to core only if/when it is
  productized as a crate with an upstream story.
- abgen -> core (absent from the starting judgment): asset-bundle
  CDN/registry/converter is platform infrastructure upstream
  (`asset-bundle-converter`); clients depend on it, so it travels with core.
  `abgen-compare` stays overlay — it is a QA harness, not a surface.
- presence, governance -> core (absent from the starting judgment): both are
  ports of upstream platform surfaces. Governance carried the one accepted
  core→lore exception below, closed 2026-07-31.
- squid-api -> core despite retirement: scope records which family a port
  belongs to; the reserved port stays classified with its chain-ingest siblings.

## Known exceptions

- CLOSED 2026-07-31 — catalyrst-governance read lore `:5433`
  (discourse/snapshot archives), optionally. This violated the "core never
  depends on overlay/lore" rule; accepted while the only archivers for
  those datasets lived in lore. Closed by the "replace with a core-owned
  ingest" path: the lore Python scrapers were ported into
  `umbrella/scripts/{discourse,snapshot}-archive.py` with timer units
  `umbrella-discourse-archive` / `umbrella-snapshot-archive`
  (`deploy/systemd/`), the `discourse` + `snapshot` databases were provisioned
  on the umbrella cluster `:5434` (`scripts/bootstrap-{discourse,snapshot}.sh`)
  and backfilled 1:1 from `:5433`, and `SNAPSHOT_DATABASE_URL` /
  `DISCOURSE_DATABASE_URL` now point at the `:5434` copies. The lore-side
  scraper units were retired (lorebook still reads its own frozen `:5433`
  copies until it is repointed like mana-price was). No core service reads
  lore anymore; the rule now holds without exceptions.

## External ports

The `external` set in `ports.nix` lists non-catalyrst listeners on this box
(lore overlay services like forgejo/code-intel/lorebook, dev servers).
They are int-valued by design and carry no `scope` field: not part of the
catalyrst product at all, de facto all overlay or dev tooling. Do not move a
service into `external` to dodge classification — anything with an
`umbrella-*` unit or a catalyrst crate belongs in the `umbrella` set (livekit
was moved out of `external` for exactly this reason: it has
`umbrella-livekit.service` and core comms depends on it).

## Rule for adding a new service

1. Every new `umbrella` entry in `deploy/ports.nix` MUST declare
   `scope = "core" | "overlay" | "instance-infra"` — `check-ports.py` fails
   otherwise.
2. Default to overlay. A service is core only if a stranger running their
   own realm needs it: it serves a client- or peer-facing surface of the realm
   product (usually a port of an upstream `decentraland/*` service, or a supply
   the shipped client consumes).
3. Core may depend on core and instance-infra only — never on overlay, never on
   lore (`:5433`). If you need an exception, it goes in "Known exceptions" above
   with a rationale and an exit plan, in the same commit.
4. Add the service to the table above with a one-line rationale, in the same
   commit that adds the port.
