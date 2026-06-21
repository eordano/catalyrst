# Parity audit — `catalyrst-explorer-api` (service "explorer-api")

Adversarial re-verification of the flagged parity findings. Each flagged shape diff
and each "better"/"worse" efficiency claim was checked against **both** the Rust
source and the upstream TS source, and cross-checked against what the Unity client
actually reads (net-catalog + the client's deserialization structs).

Crate: `crates/catalyrst-explorer-api/`
Upstream: `realm-provider`, `auth-server` (auth-api), `builder-server`,
`worlds-content-server`, plus the CDN contracts for `feature-flags` and
`config/denylist.json`.

Live references consulted: lamb2 `/about` at `127.0.0.1:5142`; live CDN
`config.decentraland.org/denylist.json`.

## Headline corrections to the incoming findings

1. **`expiration` is NOT a type diff.** Every auth endpoint flagged with
   "POTENTIAL TYPE DIFF: ours expiration=ISO string, upstream=number" is wrong.
   Upstream types `expiration: Date` and sends it via Express `res.json(...)`
   (`auth-server/src/ports/server/component.ts:60-62`), which `JSON.stringify`s a
   `Date` to an **ISO-8601 string** — identical wire encoding to our chrono
   `DateTime<Utc>` (RFC3339). **REJECTED** on POST /auth/requests,
   GET /auth/v2/requests/{id}, POST /auth/identities.

2. **`/denylist.json` is divergent, not a match.** The Unity client parses the
   denylist into `BlocklistData { users: List<User{ wallet }> }` and reads
   `users[].wallet` (`ApplicationBlocklistGuard.cs:41-46`). The canonical CDN file
   (verified live) is `{ "users": [ { "wallet": "0x..." } ] }` — `users` is an
   array of **objects**. Ours serializes `users` as `Vec<String>` (bare strings)
   plus five extra arrays (names/contents/scenes/deployments/wearables) that are a
   different (catalyst content-denylist) concept. On the field the client reads,
   the element type mismatches. Empty state (`[]`) coincidentally serializes the
   same, so it is a **latent** break that fires the moment a wallet is denied.
   Verdict changed match -> divergent.

3. **Upstream realm-provider `/about`, `/realms`, `/hot-scenes` are NOT
   "cached, background-refreshed".** Only the DAO address list is LRU-cached (1-day
   TTL, `realm-provider/src/adapters/realm-provider.ts:53-65`).
   `getHealhtyCatalysts()` then fetches `/about` from **every** catalyst on every
   request (`Promise.all`, 1s timeout each), and `mainRealmProvider.getStatus()`
   does two more fetches per call (`main-realm-provider.ts:44-46`). So our
   in-memory synthesis is even more clearly "better" than the findings stated — the
   upstream cost is a per-request network fan-out, not a cache read.

4. **Builder items is batched, not "N+1".** `Bridge.consolidateItems`
   (`builder-server/src/ethereum/api/Bridge.ts:265-324`) does one
   `Collection.findByIds` + one `peerAPI.fetchItems` (both batched) then an
   in-memory merge loop. The loop's `remoteItems.find(...)` makes it O(N^2) CPU but
   there is **no per-item I/O**. The "worse" verdict still holds, but on the correct
   basis: we add a full proxy round-trip on top of upstream's Postgres + subgraph +
   ItemCuration + batched-consolidate cost, with no local cache.

5. **`comms` sub-shape on `/about` is more nuanced than stated.** lamb2's bare
   `/about` (verified live) has **no `comms` block at all** and a bare `lambdas:
   {healthy, publicUrl}` (no version/commitHash). realm-provider's `/main/about`
   *replaces* comms with `{version, commitHash, healthy, protocol, usersCount,
   adapter}` — note **no `fixedAdapter`**. Ours emits `comms.{healthy, protocol,
   fixedAdapter, adapter, usersCount}`. The Unity client's `CommsInfo` proto
   (`About.gen.cs:92`) has exactly `Healthy, Version, CommitHash, PublicUrl,
   Protocol, UsersCount, FixedAdapter` and the client **reads `fixedAdapter`** to
   resolve the LiveKit URL (net-catalog confirms). So ours emitting `fixedAdapter`
   is actually *better aligned* to the Unity client than realm-provider's own
   `/main/about`; missing comms.version/commitHash is harmless (proto3 defaults).

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /ping | match | same | none | `/ping` literal == upstream `url.pathname`. |
| GET /main/about (alias /about) | divergent (tolerated) | better | minor | Extra `lambdas.version/commitHash`, comms has `fixedAdapter` but no version/commitHash. All are valid client proto fields -> client-tolerant. Ours emits `fixedAdapter` which the client reads; realm-provider's about does not. Upstream is per-request catalyst fan-out, ours is in-mem. |
| GET /realms | match | better | minor | `{serverName,url,usersCount}[]`; ours single static entry. Upstream fans out `/about` per catalyst per request. Data, not shape, differs. |
| GET /hot-scenes | match | better | minor | Empty `[]` valid HotSceneInfo[]. Upstream heavy fan-out (per-catalyst /stats/parcels + archipelago /parcels + fetchScenes). Client actually reads hot-scenes from archipelago-ea-stats, not here. Cost win, data-empty. |
| GET /status | divergent | same | minor | Upstream returns ONLY `{version,currentTime,commitHash}`. Ours adds `healthy,name,lastUpdate,env`. Additive; canonical 3 keys present and correctly typed (currentTime number both). |
| GET /auth/health/live | match | same | none | `{timestamp:number}` both. |
| POST /auth/requests | divergent | better | minor | Extra `challenge` (additive). `expiration` is ISO string both sides (NOT a type diff). In-mem DashMap vs Redis setRequest. |
| GET /auth/requests/{id} | match | better | none | Same 404/410/204/200 state machine, mark-fulfilled one-shot. DashMap vs 2 Redis ops. 204-with-body is cosmetic (clients ignore 204 body). |
| POST /auth/requests/{id} | match | better | none | catalyrst-only HTTP alias of v2 outcome; 200 no body; state machine matches. In-mem vs Redis. |
| GET /auth/v2/requests/{id} | divergent | better | minor | Extra `challenge` (additive). `sender` omitted-when-None matches. `expiration` ISO string both (NOT a type diff). DashMap vs Redis getRequest. |
| POST /auth/v2/requests/{id}/outcome | match | better | none | 200 empty body; 404/410/400 matches. Missing upstream socket.io relay fast-path (behavior, not shape). DashMap vs 2 Redis ops. |
| POST /auth/v2/requests/{id}/validation | match | better | none | 204 no body; flag set. Missing optional socket emit (behavior, not shape). |
| GET /auth/v2/requests/{id}/validation | match | better | none | `{requiresValidation:bool}` exact. DashMap vs Redis. |
| POST /auth/identities | match (body) | better* | major | 201 `{identityId,expiration}` matches (expiration ISO string both). *SECURITY: ours skips ALL upstream validation (signed-fetch ADR-44 middleware, validateAuthChain, ephemeral==finalAuthority, requestSender==owner, privkey->address, client-IP capture). "better" is largely because required work is omitted. |
| GET /auth/identities/{id} | match (200 body) | better* | major | 200 `{identity}` + consume-once matches. ERROR MODEL diverges: upstream has consumed/expired/ip_mismatch/evicted tombstones + 403 on IP mismatch (non-mobile); ours = 400/404/410, no IP enforcement (ip_address never captured at POST). Client polls this endpoint. |
| GET /denylist.json | **divergent** | worse | minor->moderate | `users` element type mismatch: ours `Vec<String>` vs client-read `[{wallet}]` (`BlocklistData`). Latent break (empty `[]` matches today). Reads+parses file per request (no cache) vs static CDN. |
| GET /v1/collections/{id}/items | match | worse | none | Reverse-proxy; FullItem[] verbatim. Adds proxy hop + no cache on top of upstream Postgres+subgraph+batched-consolidate (O(N^2) merge, not N+1). 502 envelope only on error path. |
| GET /v1/storage/contents/{hash} | divergent | worse | minor | Upstream `res.redirect(url,301)` to S3 (no body). Ours (reqwest follows redirects by default — no `.redirect()` override in main.rs) streams the object with 200. 200+body vs 301+Location; bypasses CDN cache. |
| GET /world/{name}/about | match | worse | none | Reverse-proxy verbatim About; upstream nameDenyListChecker + getMetadataForWorld (Postgres/storage). Proxy hop + no cache. |
| GET /world/{name}/permissions | match | worse | none | Proxy verbatim; upstream Promise.all of 3 component reads. Proxy hop + no cache. |
| POST /worlds/{name}/comms | match | worse | none | Forwards signed-fetch headers (required: upstream signedFetchMiddleware) + body; streams `{fixedAdapter}` incl 429. Proxy hop on per-request LiveKit minting (correctly uncached). |
| POST /worlds/{name}/scenes/{scene_id}/comms | match | worse | none | Same proxy + header forwarding; same upstream `worldCommsHandler`. Proxy hop. |
| GET /contents/{hash} | match | worse | none | Proxy streams; forwards Range + conditional headers; preserves content-type/length/range/accept-ranges/cache-control/etag. Upstream does native Range (200/206/416). Every byte transits our hop; no cache. |
| HEAD /contents/{hash} | match | worse | none | Proxy HEAD; upstream `storage.fileInfo`. Metadata-only hop, no cache. |
| GET /wallet/{wallet}/connected-world | match | worse | none | Proxy `{wallet,world}` verbatim. Upstream is O(1) `peersRegistry.getPeerWorld` (no DB). Full round-trip over an in-mem lookup — strictly worse. |
| GET / (feature-flags: all) | **divergent** | better (moot) | major | Default `{name,version,features:[],flags:{}}` vs client `FeatureFlagsResultDto {flags, variants}` — ours lacks `variants`, has extra `features`. PATH MISMATCH: client GETs `/{appName}.json` (e.g. `/explorer.json`), ours serves `/` -> route unreachable by real client. "better" serve cost is moot since unreachable. |
| GET /flags/{name} (single flag) | unknown | better | none | No upstream equivalent (CDN has no per-flag GET). catalyrst-only convenience. In-mem snapshot + linear scan. |

## Confirmed shape issues (survived verification)

- **GET /denylist.json — DIVERGENT (raised from "match").** `users` is `Vec<String>`
  in ours but `[{wallet}]` in the contract the Unity client deserializes
  (`BlocklistData`), and the client reads `users[].wallet`. Latent client break.
  Severity moderate (client-read field, currently masked by empty state).
- **GET / (feature-flags) — DIVERGENT, major.** Path/filename contract mismatch
  (`/explorer.json` vs `/`) makes the route unreachable by the real client; default
  payload also lacks `variants` and carries an extra `features` array.
- **POST /auth/identities — major (security/validation gap).** Success body shape
  matches, but ours omits the entire upstream validation chain (ADR-44 signed-fetch
  middleware, auth-chain, ephemeral/finalAuthority/owner/privkey checks, client-IP
  capture). Confirmed against `auth-server` lines 936-1034.
- **GET /auth/identities/{id} — major (error-model + IP enforcement gap).** 200 body
  matches; non-success state machine and IP enforcement absent. ip_address is stored
  empty at POST, so IP checks could not work even if added. Client polls this route.
- **GET /status — divergent (minor).** Extra `healthy/name/lastUpdate/env` over the
  canonical `{version,currentTime,commitHash}`. Additive.
- **GET /main/about — divergent (minor, client-tolerant).** Field set differs from
  both lamb2 and realm-provider, but every field is a valid client About proto field
  and the client tolerates the extras/absences. Emitting `comms.fixedAdapter` is
  actually beneficial for the Unity client.
- **POST /auth/requests, GET /auth/v2/requests/{id} — divergent (minor).** Extra
  `challenge` field; additive, ignored by JSON consumers (the requests flow is driven
  by the auth webapp in a browser, not the Unity client directly).

## Confirmed efficiency wins (with structural reason)

- **GET /main/about — better.** Fully synthesized from in-memory `Config`, zero
  per-request I/O. Upstream does a per-request network fan-out: `/about` to every
  healthy catalyst (`Promise.all`, 1s timeouts) + two `getStatus` fetches. Structural,
  not language-based. (Stronger than the original finding, which mis-stated upstream
  as cached.)
- **GET /realms — better.** Single static element vs upstream per-request catalyst
  fan-out + getStatus. Structural.
- **GET /hot-scenes — better.** Returns `[]`; upstream fans out per-catalyst
  /stats/parcels + archipelago /parcels + `content.fetchScenes` + sort/slice top 100.
  Cost win only (data-empty); client reads real hot-scenes elsewhere.
- **All /auth/* in-memory endpoints — better.** DashMap get/get_mut/insert/remove vs
  Redis getRequest/setRequest/setIdentity (1-2 network round-trips per op). Verified
  upstream uses a Redis-backed `storage` component. Structural I/O reduction. For the
  two identity endpoints the win is partly because required validation/tombstone work
  is omitted (see security findings).

## Rejected during verification

- "expiration type diff (ISO string vs number)" on POST /auth/requests,
  GET /auth/v2/requests/{id}, POST /auth/identities — REJECTED. Upstream `Date`
  serializes to an ISO-8601 string via `res.json`, identical to our chrono RFC3339.
- "GET /denylist.json shape matches" — REJECTED. Element type mismatch on the
  client-read `users` field (`Vec<String>` vs `[{wallet}]`); reclassified divergent.
- "upstream realm-provider about/realms/hot-scenes are cached/background-refreshed" —
  REJECTED. Only the DAO address list is cached; `/about` and `getStatus` fetches run
  per request. (Does not change the "better" verdict — strengthens it.)
- "builder items is N+1-prone per-item merge" — REJECTED as stated. The consolidate
  path is batched (`findByIds` + `fetchItems` once); the per-item loop is O(N^2) CPU
  with no I/O. "worse" verdict retained on the correct basis (added proxy hop + no
  cache).
- "comms block matches realm-provider's merged shape" — PARTIALLY REJECTED. Ours
  differs from realm-provider's comms (we add `fixedAdapter`, drop version/commitHash),
  but this is client-tolerant and our `fixedAdapter` is what the Unity client reads,
  so the net effect is not a regression.
