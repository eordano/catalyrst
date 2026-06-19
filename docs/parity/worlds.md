# Parity report — `catalyrst-worlds` (service "worlds")

Upstream: `decentraland/worlds-content-server` (TS). Rust crate: `crates/catalyrst-worlds`.

Adversarial re-verification of the flagged parity findings. Each shape divergence was
cross-checked against the **Unity client's actual deserialization model** (net-catalog +
`unity-explorer` C# models) to determine whether it affects the explorer. Each efficiency
verdict was confirmed by reading both implementations end-to-end (no language-only claims kept).

## Per-endpoint summary

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET `/world/{world}/about` | divergent | better | minor | Divergences are real vs the ADR-110 spec but the Unity `ServerConfiguration` model has **no minimap/map field** and never acts on `healthy`, so client-invisible. We do 2 SQL + 0 HTTP vs upstream ~3 SQL + cached probes. |
| GET `/world/{world}/permissions` | divergent | same | minor (was major) | `summary:{}` stub is real, but the Unity `WorldPermissionsResponse` has **no `summary` field** — client reads only `permissions.{access,deployment,streaming}` + `owner`, all of which match. Severity downgraded. |
| POST `/worlds/{world}/comms` | match | better | minor | Success `{fixedAdapter}` matches. We skip LiveKit participant-count, on-chain checkPermission, social-service, and redis hops — structurally cheaper. No 503 capacity path. |
| POST `/worlds/{world}/scenes/{scene}/comms` | match | better | minor | Same as world route + we skip the per-scene LiveKit room count and scene-ban lookup. |
| GET `/contents/{hash}` | match | worse | minor | Byte-identical streamed passthrough. We add a reverse-proxy network hop; upstream reads `IContentStorageComponent` directly + validates IPFSv2 + parses ranges + synthesizes ETag/Cache-Control/CORS. |
| HEAD `/contents/{hash}` | match | worse | minor | Same proxy-hop story; upstream does a single local `storage.retrieve` probe. |
| POST `/livekit-webhook` | divergent | same | minor | Success body `{ok:true}` (object) vs upstream raw-string body; skip messages differ in casing. **Not in the Unity net-catalog** — LiveKit-facing, no explorer impact. We skip signature verify + NATS (omitted functionality, not a win). |
| GET `/status` | divergent | better | minor (was major) | Entirely different envelope (catalyrst health vs `{content,comms}`). **Not in the Unity net-catalog** — monitoring endpoint, no explorer impact. We do 0 SQL vs upstream 1 SQL + LiveKit `commsAdapter.status()`. Severity downgraded. |

(Unflagged endpoints `GET /wallet/{wallet}/connected-world` and `GET /ping` were accepted as-is; spot-checked and consistent — `connected-world` reads a `DashMap` presence registry, parity body `{wallet,world}`.)

## Confirmed shape issues

1. **`/about` minimap is incomplete vs spec.** Upstream conditionally adds
   `configurations.minimap.dataImage` and `.estateImage` (defaulting to
   `api.decentraland.org/v1/minimap.png` / `estatemap.png`) when the minimap is enabled or
   a runtime image is set (`world-about-handler.ts:66-77`). We emit only `{enabled}`
   (`about.rs:73`). **Real divergence, but client-invisible**: the Unity `ServerConfiguration`
   model (`unity-explorer/.../ServerConfiguration.cs`) has no `minimap`/`map`/`globalScenesUrn`
   fields at all — it deserializes only `scenesUrn`, `localSceneParcels`, `realmName`,
   `networkId`, `skybox`. A non-Unity / web consumer reading the ADR-110 spec would notice.

2. **`/about` top-level `healthy`/`acceptingUsers` and `content.healthy`/`lambdas.healthy`
   are hardcoded `true`.** Upstream computes them from `status.getContentStatus()` /
   `getLambdasStatus()` (`world-about-handler.ts:56,91`). We hardcode (`about.rs:88-89,102,106`).
   **Real, but client-effectively-invisible**: top-level `healthy`/`acceptingUsers` aren't in
   the Unity `ServerAbout` model; `content.healthy`/`lambdas.healthy` ARE deserialized
   (`ContentEndpoint.cs`) but are never read/acted upon (only reset in `Clear()`).
   `content.publicUrl`/`lambdas.publicUrl` come from static config vs live status — same
   field/type, different source.

3. **`/permissions` `summary` is a `{}` stub.** Upstream returns a populated
   `Record<address, [{permission, world_wide, parcel_count?}]>` (`permissions-handlers.ts:80-87`);
   we return `{}` (`permissions.rs:35`). **Real, but client-invisible**: the Unity
   `WorldPermissionsResponse` (`unity-explorer/.../WorldPermissionsData.cs`) has no `summary`
   field. The fields the client DOES read all match: `permissions.access.{type,wallets,communities}`
   (our `to_public_json()` emits exactly these for allow-list, `access.rs:59-66`),
   `permissions.{deployment,streaming}.{type,wallets}`, and `owner`.

4. **`/livekit-webhook` success/skip bodies differ.** Success: we return `{ok:true}`
   (object), upstream returns the raw verified request-body string. Skip: we return
   lowercase `{message:"skipping event"}` / `{message:"skipping non-world room"}`, upstream
   `{message:"Skipping event"}`. **No explorer impact** — endpoint absent from the Unity
   net-catalog; it's a LiveKit server-to-server callback. Behavioral gaps (no signature
   verify, no NATS fan-out, prefix-based room gating instead of `.dcl.eth` suffix) are real
   but out of the explorer's contract.

5. **`/status` envelope is entirely different.** We emit the catalyrst health shape
   `{ok, data:{image,timestamp,version}}` (`status.rs:11-20`); upstream emits
   `{content:{commitHash,worldsCount:{ens,dcl}}, comms:{...}}` (`status-handler.ts`).
   **No explorer impact** — absent from the Unity net-catalog (monitoring endpoint).

## Confirmed efficiency wins (with structural reason)

1. **`/about` — better.** Ours: 2 SQL (`get_world` + `get_scenes`), 0 outbound HTTP,
   runtime metadata computed inline from `scenes[0]`. Upstream: ~3 SQL
   (`worlds` LEFT JOIN `blocked`, then `getWorldScenes` = `COUNT(*)` + `SELECT` via
   `Promise.all`, confirmed `worlds-manager.ts:115`/`getWorldScenes` count+select) **plus**
   `getContentStatus` + `getLambdasStatus`. Note the status probes are **cached 5 min**
   (`status.ts:15` `STATUS_EXPIRATION_TIME_MS`), so the original "two live HTTP probes on
   every request" claim is corrected — they fan out only on cache miss. Even so we are
   structurally cheaper (fewer SQL, zero HTTP). Caveat confirmed: our `get_scenes` fetches
   **all** scenes (no `LIMIT 1`, `ports/worlds.rs:89-98`) while upstream uses `limit:1` —
   a minor over-fetch.

2. **POST `/worlds/{world}/comms` — better.** Confirmed by reading `logic/comms/component.ts`:
   upstream `getWorldRoomConnectionString` runs `bans.isPlayerBanned` + `denyList.isDenylisted`
   + `isWorldValid` + `Promise.all(namePermissionChecker.checkPermission [on-chain NFT/ENS],
   access.checkAccess)` + `commsAdapter.getWorldRoomParticipantCount` (**a LiveKit API call**)
   + a **Redis-backed** rate-limiter (`rate-limiter/component.ts:18-21` takes `redis`, with
   lock acquire/retry round-trips). Ours: `get_access` + `is_world_valid` + `is_wallet_blocked`
   (3 SQL EXISTS-style), a **moka** in-process rate-limiter (`rate_limiter.rs:6`), token minted
   locally. We genuinely avoid the LiveKit round-trip, the on-chain ownership lookup, and the
   redis hop. Not a language claim. Trade-off: we omit capacity (503), owner-permission,
   community membership, and platform/scene bans beyond the local `blocked` table.

3. **POST `/worlds/{world}/scenes/{scene}/comms` — better.** Same structure plus upstream adds
   `getWorldSceneBaseParcelIncludingUndeployed` + `isUserBannedFromScene` +
   `getWorldSceneRoomsParticipantCount` (LiveKit) (`component.ts:67-90`). Ours adds one
   `get_scene_base_parcel` SQL. Confirmed cheaper.

4. **GET `/status` — better.** Ours: 0 SQL, fully static body. Upstream: 1 SQL
   (`getDeployedWorldCount`) + `commsAdapter.status()` (LiveKit). Real, but only because we
   emit a different (monitoring) body and skip the worlds-count + comms status entirely.

## Confirmed efficiency caveats (NOT wins — "worse")

- **`/contents/{hash}` GET & HEAD — worse, confirmed.** `contents.rs` is a `reqwest`
  reverse-proxy to `contents_upstream_url` (default `http://127.0.0.1:5141`, `config.rs:48`)
  using `Body::from_stream`. Every read is an extra network hop + double-streaming. Upstream
  `content-file-handler.ts` reads `IContentStorageComponent` directly (`storage.retrieve` +
  `asRawStream`), so no proxy hop. We also forward only a fixed header set and add **no**
  `Access-Control-Expose-Headers`, do **no** IPFSv2 `400` validation, and do **no** local
  range parsing (`206`/`416`) — all delegated to the proxied store. Streaming-not-buffering is
  a point in our favour, but the hop dominates. Severity minor (asset bytes are byte-identical;
  only caching/CORS/range semantics depend on the backing store).

## Rejected / corrected during verification

- **REJECTED the "major" severity on `/permissions` summary** as a client-facing issue. The
  `summary:{}` stub is a genuine spec divergence but the Unity client never deserializes a
  `summary` field, so it has zero explorer impact. Downgraded to minor (kept as a documented
  divergence for non-Unity consumers).

- **REJECTED the "major" severity on `/status`** as client-facing. Endpoint is absent from the
  Unity net-catalog; it's a monitoring endpoint. Downgraded to minor.

- **CORRECTED the `/about` efficiency rationale.** The original claim that upstream runs "two
  live HTTP health probes on every request" overstates it: `getContentStatus`/`getLambdasStatus`
  are cached for 5 minutes (`STATUS_EXPIRATION_TIME_MS`), and the name-deny-list HTTP fetch is
  also LRU-cached. The "better" verdict still holds on SQL count + zero-HTTP grounds, but not on
  a per-request fan-out basis.

- **NOTED `/permissions` is arguably cheaper than "same".** Upstream's `getOwner` internally
  calls `getMetadataForWorld` (worlds query + `getWorldScenes` count+select) plus a possible
  on-chain `nameOwnership.findOwners` fallback, on top of `getAccessForWorld` (1 SQL) and
  `getPermissionsSummary` (1 SQL) — so upstream does ~5 SQL + possible RPC, concurrently. Ours
  does 3 simple SQL (1 world + 2 sequential wallet selects), no RPC, no summary. The "same"
  verdict is conservative; kept as "same" because we skip the summary work the {} stub
  represents, but our raw per-request cost is lower. Minor inefficiency on our side: the two
  wallet selects run sequentially and could be one `IN (...)` query.

- **No findings rejected on language-only grounds.** Every "better"/"worse" verdict was traced
  to a concrete structural difference (extra/absent SQL, LiveKit call, redis hop, on-chain
  lookup, or proxy hop) in both codebases.
