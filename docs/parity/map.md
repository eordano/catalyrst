# Parity report — `catalyrst-map` (service "map")

Upstream: `decentraland/atlas-server` (`decentraland/atlas-server`).
Our crate: `crates/catalyrst-map`.

**Client-impact note (verified against the Unity net-catalog):** the Unity explorer
calls **only `GET /v1/map.png`** from atlas-server
(`api.decentraland.{ENV}/v1/map.png?center={x,y}&width=&height=&size=`). It does NOT
call `/v2/tiles`, `/v1/tiles`, `/v2/parcels`, `/v2/estates`, `/v2/contracts/.../tokens`,
`/v2/districts`, or `/contributions`. Those NFT-metadata / tiles JSON endpoints are
consumed by the marketplace dApp and OpenSea-style NFT viewers. So every shape
divergence confirmed below is a **dApp/marketplace/NFT-viewer parity** concern, **not**
an explorer-visible one. (`crates/catalyrst-map/src/handlers/meta.rs` says as much in its
own header.)

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /v1/map.png | match | same | minor | PNG bytes; no `listed-for-rent` overlay (pixels only). Not a shape diff. CLIENT-FACING (only endpoint the explorer uses). |
| GET /v2/map.png | match | same | minor | Alias of v1 handler. |
| GET /v1/parcels/{x}/{y}/map.png | match | same | minor | In-memory render, 0 queries. |
| GET /v2/parcels/{x}/{y}/map.png | match | same | minor | Alias of v1. |
| GET /v1/estates/{id}/map.png | match | same | minor | 302 to `dissolved_estate.png` on empty; selection/center nuances are framing-only. |
| GET /v2/estates/{id}/map.png | match | same | minor | Alias of v1. |
| GET /v1/tiles | divergent | mixed (contested "better") | minor | Missing `rentalPricePerDay` (only on rented tiles). Data-pipeline cheaper; per-request cache absent. |
| GET /v2/tiles | divergent | mixed (contested "better") | minor | Missing `rentalListing`; **`exclude=` param really diverges** (ours applies, upstream is a no-op). |
| GET /v2/tiles/info | match | same | none | `{lastUpdatedAt}`, `no-cache` both. |
| GET /v2/parcels/{x}/{y} | divergent | worse | minor | Missing proximity attributes. 1 SQL/request vs upstream 0. |
| GET /v2/estates/{id} | divergent | worse | minor | Missing proximity attrs + **no dissolved-estate fallback** (404 vs upstream 200). |
| GET /v2/contracts/{address}/tokens/{id} | divergent | worse | minor | Missing `cache-control: immutable` for LAND; missing dissolved-estate fallback; up to 2 SQL vs 0. |
| GET /v2/districts | match | same | none | Same static `districts.json`. |
| GET /v2/districts/{id} | divergent | same | minor | **404 body differs**: ours JSON `{ok:false,error:"Not Found"}`, upstream plain string `Not found`. |
| GET /v2/addresses/{address}/contributions | match | same | minor | Key-casing leniency only; identical for lowercase addresses. |
| GET /ping, /v2/ping, /ready, /v2/ready | match | same | none | Extra `/ping` `/ready` aliases additive. |

## Confirmed shape issues

All verified by reading both the Rust struct/handler and the upstream TS shape.

1. **`/v2/districts/{id}` 404 body diverges (CONFIRMED).**
   - Upstream `routes.ts:126` returns `{ status: 404, body: 'Not found' }` — a plain
     string (text content-type), capital N lowercase f.
   - Ours `handlers/meta.rs:171` `get_district` returns `not_found()` =
     `Json({ "ok": false, "error": "Not Found" })` (application/json).
   - Different content-type AND different payload on the miss path. Clients that
     string-match the body or branch on content-type see a difference. Severity minor;
     not explorer-facing.

2. **`/v2/parcels/{x}/{y}` and `/v2/estates/{id}` drop proximity attributes (CONFIRMED).**
   - Upstream `api/component.ts buildParcel` (L618-640) / `buildEstate` (L661-677) append
     `Distance to <Plaza|Road|District>` attribute rows via `getProximity(coords)`.
     `proximity.json` has **34,888** coordinate entries — a large fraction of the ~90k
     grid carries at least one distance trait.
   - Ours emits ONLY `[X, Y]` (parcels, `meta.rs:73-76`) / `[Size]` (estates,
     `meta.rs:120-122`). No `getProximity` port exists.
   - Real metadata gap for NFT viewers. X/Y/Size trait rows themselves match exactly.

3. **`/v2/estates/{id}` and estate-contract `/v2/contracts/.../tokens/{id}` lack the
   dissolved-estate fallback (CONFIRMED).**
   - Upstream `estateRequestHandler` (handlers.ts:343-354) and `tokenRequestHandler`
     (L386-391) fall back to `map.getDissolvedEstate(id)` on an in-memory miss and can
     return a **200 NFT** (size-0 estate, fetched from subgraph and cached).
   - Ours `get_estate` returns `not_found()` on miss; `get_token` for the estate
     contract delegates to `get_estate` with no fallback. So a dissolved estate that
     upstream resolves to 200 returns **404** from us. Observable behavioral divergence.

4. **`/v2/contracts/.../tokens/{id}` missing `cache-control` for LAND tokens (CONFIRMED).**
   - Upstream `tokenRequestHandler` (handlers.ts:376-378) sets
     `cache-control: public, max-age=3600, s-maxage=3600, immutable` when the address is
     the LAND contract.
   - Ours `get_token` (`meta.rs:128-159`) delegates to `get_parcel`, which emits no
     cache-control header. LAND token metadata is immutable, so downstream/CDN caching is
     lost; clients refetch far more often.

5. **`/v2/tiles` `exclude=` param really diverges (CONFIRMED — and subtle).**
   - Upstream `getFilterFromUrl` (filter-params.ts:62) guards the exclude branch with
     `else if (exclude && result.length > 0)`. `result` is a `Record<string,Tile>`
     (plain object) with no `.length`, so `result.length` is `undefined` and the branch is
     **dead code** — `?exclude=` is a NO-OP and upstream returns full tiles.
   - Ours `filter_tiles` (`handlers/tiles.rs:79-92` `project_exclude`) actually strips the
     excluded fields. So for `?exclude=...` requests our output omits fields while upstream
     returns everything. Real divergence on the exclude param.
   - The `include=` param, by contrast, is net-identical (upstream writes `undefined`
     values that `JSON.stringify` drops; ours only inserts present keys).

6. **`/v1/tiles` missing `rentalPricePerDay`; `/v2/tiles` missing `rentalListing` (CONFIRMED, low impact).**
   - Upstream `legacy-tiles.ts:45-50` adds `rentalPricePerDay` (max of
     `rentalListing.periods[].pricePerDay`) only when `tile.rentalListing` is set; the v2
     `Tile` type can carry `rentalListing` (built in `api/component.ts:600-602`).
   - Our `Tile` struct (`map.rs:40-66`) has no `rentalListing` field and we never ingest
     rentals, so both are always absent. For all non-rented tiles the shapes are
     byte-identical. Only affects tiles with an active rental listing.

## Confirmed efficiency findings (with structural reason)

- **`/v2/parcels/{x}/{y}` — WORSE (CONFIRMED).** Ours runs 1 on-demand SQL per request
  (`meta.rs:53-60`, `fetch_optional`), no cache. Upstream `getParcel`
  (`map/component.ts:373-380`) reads from a precomputed in-memory `Record<string,NFT>`
  (`parcels` future) built during `fetchData`/poll = **0 queries/request**. Structural,
  not language-based.

- **`/v2/estates/{id}` — WORSE (CONFIRMED).** Ours = 1 SQL/request. Upstream `getEstate`
  (`component.ts:382-385`) = 0 queries on the in-memory hit; only a MISS may fire 1
  subgraph query (`getDissolvedEstate`, itself memoized via `notFoundDissolvedEstateIds`
  set and a `lastEstateId` short-circuit, L394-431).

- **`/v2/contracts/.../tokens/{id}` — WORSE (CONFIRMED).** Ours = up to 2 SQL (LAND: 1 to
  map tokenId→x,y then `get_parcel`'s +1; estate: 1 via `get_estate`), no cache, and no
  immutable header so downstream caches can't absorb repeats. Upstream `getToken`
  (`component.ts:433-438`) = 0 queries on the in-memory `tokens` map hit + the immutable
  cache header for LAND.

## Contested / downgraded verdicts (rejected as clean "better")

- **`/v1/tiles` and `/v2/tiles` "better" → downgraded to MIXED.** The "better" rested
  only on the **data-refresh** axis, and that part is real: our `build()` (`map.rs:146-182`)
  issues a **single bulk SQL JOIN** (`query_as::fetch_all`, one round-trip) against the
  local squid DB; upstream `fetchData` (`api/component.ts:119-172`) pages the theGraph
  subgraph in a `while (!complete)` loop (`first: batchSize, skip: batchSize*page`) with
  concurrency batching across ~90k parcels, plus an external subgraph dependency. That is a
  structural pipeline win, not a language artifact.
  BUT on the **per-request** axis upstream is actually better: it wraps both tiles handlers
  in `cacheWrapper([map.getLastUpdatedAt])` (`handlers.ts:12,54`; `cache-wrapper.ts`) which
  **memoizes the full serialized response per URL** between map refreshes. Ours
  re-serializes the entire (potentially ~90k-entry) object on every call with no
  per-request memoization. For a hot, large-payload endpoint this recurring serialization
  cost is meaningful. Net: a genuine trade-off, not a clean win — so I am NOT confirming an
  unqualified efficiency win here. Recommended fix: add a `lastUpdatedAt`-keyed response
  cache on our side to match upstream's memoization.

## Rejected during verification

- "include projection diverges" — REJECTED. Upstream writes `undefined`-valued keys that
  `JSON.stringify` drops, yielding output identical to ours (which only inserts present
  keys) for valid fields. Net-identical; not a divergence.
- "legacy `type` always present vs conditional" — REJECTED as non-issue. Upstream guards
  `if (tile.type)`, but every tile always has a type in practice, so the field is always
  emitted on both sides with the identical numeric mapping (10/5/9/11/8/7).
- Any explorer-facing impact for the divergent JSON endpoints — REJECTED. Net-catalog
  confirms the Unity client only fetches `/v1/map.png`; the proximity/dissolved/cache-header/
  exclude/district-404 gaps cannot affect the explorer. They remain valid dApp/NFT-viewer
  parity gaps.
