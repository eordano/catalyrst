# Endpoint map — `catalyrst-map` (service "map", bundle: explore 5143)

Committed tree, branch `feat/service-plane-crates`.
Our crate: `crates/catalyrst-map`.
Upstream: `decentraland/atlas-server` (`decentraland/atlas-server`).

## Routing tables

- Health routes mounted in `src/main.rs:24-27` (`/ping`, `/ready`, `/v2/ping`, `/v2/ready`).
- All API routes in `src/lib.rs:87-127` (`api_router`).
- Upstream router: `src/controllers/routes.ts:37-146`; handlers in `src/controllers/handlers.ts`.

## Client-impact note (verified against the Unity net-catalog)

The Unity explorer calls **only `GET /v1/map.png`** from atlas-server. The sole call
site is `ParcelChunkController.cs:65-66` via `IWebRequestController.GetTextureAsync`
(net-catalog `endpoints` row: `ParcelChunkController.cs:66`, body_shape
`query:center,width,height,size`, call_kind `IWebRequestController`). The URL base is
`DecentralandUrl.ApiChunks` (enum `DecentralandUrl.cs:30`), resolved to
`https://api.decentraland.{ENV}/v1/map.png` in `DecentralandUrlsSource.cs:187`. The
response is consumed as a raw PNG **Texture2D** (no JSON DTO/converter) and used for
parcel-atlas chunk textures. No other map/tiles/parcels/estates/districts/contributions
endpoint appears anywhere in the catalog — those JSON/NFT-metadata endpoints are
dApp/marketplace/NFT-viewer surfaces, not explorer-visible.

## Per-endpoint detail

### PNG renderers (in-memory render, 0 queries/request)

| Endpoint | Our handler | Upstream handler |
|---|---|---|
| GET /v1/map.png | `handlers/map_png.rs:80` `map_png` (route `lib.rs:89`) | `handlers.ts:148` `mapPngRequestHandler` (route `routes.ts:49`) |
| GET /v2/map.png | `handlers/map_png.rs:80` `map_png` (route `lib.rs:90`) | `handlers.ts:148` `mapPngRequestHandler` (route `routes.ts:80`) |
| GET /v1/minimap.png | `handlers/map_png.rs:190` `minimap_png` (route `lib.rs:91`) | `handlers.ts:92` `miniMapHandler` (route `routes.ts:54`) |
| GET /v1/estatemap.png | `handlers/map_png.rs:200` `estate_minimap_png` (route `lib.rs:93`) | `handlers.ts:120` `estateMapHandler` (route `routes.ts:62`) |
| GET /v1/parcels/{x}/{y}/map.png | `handlers/map_png.rs:102` `parcel_map_png` (route `lib.rs:97`) | `handlers.ts:196` `parcelMapPngRequestHandler` (route `routes.ts:70`) |
| GET /v2/parcels/{x}/{y}/map.png | `handlers/map_png.rs:102` `parcel_map_png` (route `lib.rs:101`) | `handlers.ts:196` `parcelMapPngRequestHandler` (route `routes.ts:85`) |
| GET /v1/estates/{estate_id}/map.png | `handlers/map_png.rs:130` `estate_map_png` (route `lib.rs:105`) | `handlers.ts:244` `estateMapPngRequestHandler` (route `routes.ts:75`) |
| GET /v2/estates/{estate_id}/map.png | `handlers/map_png.rs:130` `estate_map_png` (route `lib.rs:109`) | `handlers.ts:244` `estateMapPngRequestHandler` (route `routes.ts:90`) |

Notes:
- `/v1/map.png` is the only client-facing route (texture, not JSON). Params parsed in
  `extract_params` (`map_png.rs:29-58`): width/height clamped 100..4096, size 5..50,
  center, `on-sale`, `selected` (`;`-separated). Upstream `extractParams`
  (`filter-params.ts:82`) additionally reads `show-listed-for-rent`; ours parses no
  `show-listed-for-rent` and never overlays a listed-for-rent layer (pixel-only diff,
  not a shape diff; not client-observable for the explorer which sends only
  center/width/height/size).
- Estate PNG empty-set fallback: ours returns **302 → `ui.decentraland.org/dissolved_estate.png`**
  (`map_png.rs:156-165`), matching upstream `handlers.ts:261-269`.
- `/v1/minimap.png` + `/v1/estatemap.png` now exist on our tree (prior report omitted
  them). Upstream wraps these two with a 600s maxAge/staleWhileRevalidate cache header
  middleware (`routes.ts:55-58, 63-66`); ours emits plain `image/png` with no
  cache-control (`map_png.rs:67-74`).

### Tiles JSON

| Endpoint | Our handler + response | Upstream handler + DTO |
|---|---|---|
| GET /v1/tiles | `handlers/tiles.rs:149` `get_legacy_tiles`; body `{ok,data:{id->LegacyTile}}` via `to_legacy` (`tiles.rs:117`) | `handlers.ts:50` `createLegacyTilesRequestHandler` → `toLegacyTiles` (`adapters/legacy-tiles.ts:25`); `LegacyTile` type `modules/map/types.ts:99` |
| GET /v2/tiles | `handlers/tiles.rs:78` `get_tiles`; body `{ok,data:{id->Tile}}`; `Tile` struct `map.rs:33-62` | `handlers.ts:8` `createTilesRequestHandler`; `getFilterFromUrl` (`filter-params.ts:5`); `Tile` type `modules/map/types.ts` |
| GET /v2/tiles/info | `handlers/tiles.rs:178` `tiles_info`; body `{lastUpdatedAt}`, `cache-control: no-cache` | `handlers.ts:416` `tilesInfoRequestHandler` (route `routes.ts:47`) |

Notes:
- Our `Tile` struct (`map.rs:33-62`) now carries `rentalListing` (`map.rs:60-61`,
  `rentals::TileRentalListing`), and `to_legacy` emits `rentalPricePerDay`
  (`tiles.rs:143-145`) — matches upstream `legacy-tiles.ts:45-46` and the v2 Tile
  `rentalListing` field. Whether values populate depends on the rentals ingest
  (`src/rentals.rs`); for non-rented tiles shapes are byte-identical.
- Both tiles handlers now have a per-request response cache keyed by
  `v1?{raw}` / `v2?{raw}` (`tiles.rs:86-89, 99-100, 157-160, 173-174`) invalidated by
  `lastUpdatedAt` — matches upstream `cacheWrapper([map.getLastUpdatedAt])`
  (`handlers.ts:12, 54`).
- `include=` filtering: ours `project_include` (`tiles.rs:66`) + `VALID_FIELDS`
  whitelist (`tiles.rs:22-25`). Upstream `getFilterFromUrl` (`filter-params.ts:47-67`).
  `exclude=` param: upstream branch `else if (exclude && result.length > 0)`
  (`filter-params.ts:62`) is dead code (`result` is a plain object, no `.length`), so
  upstream `exclude` is a no-op. **Ours does not implement `exclude` at all**
  (`filter_tiles` only handles bbox + include) — both effectively ignore `exclude`, so
  this matches now (prior report flagged a divergence that no longer exists).
- Upstream tiles handlers also have an `ATLAS_REDIRECT_TO_S3` feature-flag branch
  returning 301 to a pre-uploaded S3 URL (`handlers.ts:18-34, 60-76`). Ours has no
  feature-flag/S3 redirect path (always serves computed JSON).

### NFT metadata JSON (not client-called)

| Endpoint | Our handler + response | Upstream handler + DTO |
|---|---|---|
| GET /v2/parcels/{x}/{y} | `handlers/meta.rs:24` `get_parcel`; NFT json built `meta.rs:64-72` | `handlers.ts:302` `parcelRequestHandler` → `map.getParcel` → `buildParcel` (`modules/api/component.ts:607`) |
| GET /v2/estates/{id} | `handlers/meta.rs:76` `get_estate` → `build_estate_nft` (`meta.rs:93`) / `build_dissolved_estate` (`meta.rs:139`) | `handlers.ts:327` `estateRequestHandler` → `getEstate`/`getDissolvedEstate`; `buildEstate` (`modules/api/component.ts:653`) |
| GET /v2/contracts/{address}/tokens/{id} | `handlers/meta.rs:175` `get_token` | `handlers.ts:357` `tokenRequestHandler` → `map.getToken` |
| GET /v2/districts | `handlers/meta.rs:224` `get_districts`; `{ok,data}` | inline route `routes.ts:111-114` → `district.getDistricts()` |
| GET /v2/districts/{id} | `handlers/meta.rs:228` `get_district`; 404 body `"Not found"` (`meta.rs:231`) | inline route `routes.ts:115-132`; 404 body `'Not found'` (`routes.ts:128`) |
| GET /v2/addresses/{address}/contributions | `handlers/meta.rs:235` `get_contributions`; `{ok,data}` | inline route `routes.ts:134-144` → `district.getContributionsByAddress` |

Notes (current tree closed several prior gaps):
- **Proximity attributes now ported** — `proximity::append_attributes` is called for
  parcels (`meta.rs:62`) and estates (`meta.rs:126`), matching upstream `getProximity`
  appended in `buildParcel` (`component.ts:631`) / `buildEstate` (`component.ts:668`).
- **Dissolved-estate fallback now present** — `get_estate` falls back to
  `build_dissolved_estate` on miss returning 200 (`meta.rs:86-89`), matching upstream
  `estateRequestHandler` (`handlers.ts:348-351`); same for the estate-contract branch of
  `get_token` (`meta.rs:215-217`) vs upstream `tokenRequestHandler` (`handlers.ts:386-391`).
- **LAND token immutable cache-control now present** — `get_token` injects
  `cache-control: public, max-age=3600,s-maxage=3600, immutable` for the LAND contract
  (`meta.rs:198-204`), matching upstream `handlers.ts:376-378`.
- **District 404 body now matches** — ours returns plain `"Not found"` (`meta.rs:231`),
  matching upstream's plain string (`routes.ts:128`). (Prior report flagged JSON
  mismatch; fixed.)
- Per-request data source diverges (structural, not client-facing): `get_parcel`
  (`meta.rs:36-52`), `build_estate_nft` (`meta.rs:96-121`), `get_token`
  (`meta.rs:185-193`) each issue on-demand SQL against the squid schema; upstream reads
  precomputed in-memory `parcels`/`estates`/`tokens` maps (`map/component.ts`), 0
  queries on hit.
- Invalid-coord / invalid-id guards match upstream: 403 `Invalid x or y`
  (`meta.rs:31-33` vs `handlers.ts:314-316`), 403 `Invalid id` (`meta.rs:80-82` vs
  `handlers.ts:338-341`); 404 body `{ok:false,error:"Not Found"}` (`meta.rs:16-22` vs
  `handlers.ts:323, 352, 392`).

### Health / status

| Endpoint | Our handler | Upstream handler |
|---|---|---|
| GET /ping | `handlers/status.rs:7` `ping` → `"ok"` (route `main.rs:24`) | `handlers.ts:396` `pingRequestHandler` |
| GET /v2/ping | `handlers/status.rs:7` `ping` (route `main.rs:26`) | `handlers.ts:396` `pingRequestHandler` (route `routes.ts:94`) |
| GET /ready | `handlers/status.rs:11` `ready` (route `main.rs:25`) | `handlers.ts:403` `readyRequestHandler` |
| GET /v2/ready | `handlers/status.rs:11` `ready` (route `main.rs:27`) | `handlers.ts:403` `readyRequestHandler` (route `routes.ts:95`) |

`/ping` and `/ready` (no `/v2`) are additive aliases on our side; upstream only exposes
the `/v2` variants. `ready` returns 200 `"ok"` when `map.is_ready()` else 503
`"Not ready"` (`status.rs:11-17`) — matches upstream `handlers.ts:403-414`.

## Summary of remaining divergences (all dApp/NFT-viewer, none explorer-facing)

1. `/v1/map.png` (+ all png) ignore `show-listed-for-rent` overlay param — pixel-only,
   not sent by the explorer.
2. minimap/estatemap PNG lack the 600s cache-control middleware upstream sets.
3. Tiles handlers lack the `ATLAS_REDIRECT_TO_S3` 301 feature-flag branch.
4. NFT-metadata endpoints serve from on-demand SQL vs upstream's in-memory maps
   (efficiency, identical shape).

Closed since the prior parity report: proximity attributes, dissolved-estate fallback,
LAND immutable cache-control, district 404 body, tiles response cache, exclude-param
behavior (both no-op now), rentalListing/rentalPricePerDay on the Tile struct.

---

## Adversarial verification of the crate-level startup + error-model findings

Re-check findings list was empty (`[]`); the crate-level narrative was scrutinized
against the committed tree, the upstream shapes, and the Unity consumer. All claims
hold. Summary table below; details follow.

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| GET /v1/map.png | match (PNG) | none — only explorer call; all exceptions caught → `Texture2D.whiteTexture` | none | yes | 503 if grid not built; 500 render path effectively unreachable. |
| GET /v2/map.png | match | not called by explorer | none | yes | Alias of v1. |
| GET /v1,/v2/parcels/{x}/{y}/map.png | match | not called by explorer | none | yes | In-memory render. |
| GET /v1,/v2/estates/{id}/map.png | match | not called by explorer | none | yes | 302 → dissolved_estate.png on empty. |
| GET /v1/minimap.png, /v1/estatemap.png | match | not called by explorer | none | yes | In-memory. |
| GET /v1/tiles | divergent (rentals env-gated) | not called by explorer | minor | yes | In-memory snapshot + cache; no per-request DB. |
| GET /v2/tiles | divergent (rentals env-gated) | not called by explorer | minor | yes | Same; cannot 500 on DB. |
| GET /v2/tiles/info | match | not called by explorer | none | yes | 503 w/ no-cache when not ready. |
| GET /v2/parcels/{x}/{y} | divergent (proximity ported) | not called by explorer | minor | DIVERGENT | DB error swallowed `.ok().flatten()` → 404 (upstream 500). |
| GET /v2/estates/{id} | divergent | not called by explorer | minor | DIVERGENT | Dissolved fallback present. DB error → 404. |
| GET /v2/contracts/{address}/tokens/{id} | divergent | not called by explorer | minor | DIVERGENT | LAND immutable + dissolved fallback present. DB error → 404. |
| GET /v2/districts | match | not called by explorer | none | yes | Static JSON, no failure path. |
| GET /v2/districts/{id} | match (404 body fixed) | not called by explorer | none | yes | Plain `"Not found"`. |
| GET /v2/addresses/{address}/contributions | match | not called by explorer | none | yes | In-memory, always 200. |
| GET /ping,/ready,/v2/ping,/v2/ready | match | not called by explorer for map | none | yes | Additive aliases. |

### Confirmed issues (all minor, none explorer-facing)

1. **DB errors swallowed to 404 on JSON meta endpoints (CONFIRMED).**
   `get_parcel` (`meta.rs:45-52`), `build_estate_nft` (`meta.rs:106-112`),
   `build_dissolved_estate` (`meta.rs:155-160`), LAND lookup in `get_token`
   (`meta.rs:188-193`) all use `.fetch_optional(...).await.ok().flatten()`. A
   transient query failure is indistinguishable from a missing row → 404, whereas
   upstream `map.getParcel/getEstate` would propagate a subgraph error → 500.
   Degrades safely (no panic, recoverable). No explorer endpoint hits this path.

2. **PNG 500 body is a bare string, not JSON (CONFIRMED, near-dead path).**
   `map_png.rs:98,126,186,196,206` return `(INTERNAL_SERVER_ERROR, e: String)`;
   upstream `handlers.ts:184-193` returns `500 {ok:false,error}` JSON. The render
   error only fires if `Pixmap::new` fails (dims clamped 100..4096, `map_png.rs:30-32`)
   or `encode_png` fails on an in-memory pixmap — neither happens in practice.

3. **Two coexisting error-body conventions (CONFIRMED, mirrors upstream).**
   Plain-text 503/403/district-404 coexist with JSON `{ok:false,error:"Not Found"}`
   for parcel/estate/token 404s (`meta.rs:16-22`). Matches upstream's own mix
   (`handlers.ts:323,352,392` JSON; `routes.ts:128` plain string).

4. **Startup not strictly panic-free but no avoidable panics (CONFIRMED).**
   - Missing `DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING` (`config.rs:19`) → clean
     anyhow `Err`, no panic.
   - Eager `connect_with(...).await` (`lib.rs:31-42`) → clean error exit if DB
     unreachable; acceptable (nothing to serve without squid DB).
   - Initial `map.refresh()` is `match`-wrapped (`lib.rs:58-66`): on failure logs and
     serves 503 until 60s ticker succeeds. Graceful, no panic.
   - Rentals optional: `RentalsClient::from_env()` → `None` when env unset
     (`rentals.rs:77-89`); refresh-time error caught → warn, tiles served without
     rental data (`map.rs:208-214`).
   - Four `include_str!` vendored data files parsed with `.expect()` (`map.rs:110`,
     `proximity.rs:18`, `districts.rs:11,22`) — compile-time assets (all present in
     `crates/.../data/`, 0.7–6.9 MB), not runtime input; not a real failure trigger.
   - No LiveKit dependency.

### Client-crash risks

**None.** The Unity explorer's only call to this service is `GET /v1/map.png` via
`GetTextureAsync` (`ParcelChunkController.cs:66`). The consumer wraps the await in
`try/catch` (`ParcelChunkController.cs:79-89`) and degrades any failure (503 / 500 /
transport) to `Texture2D.whiteTexture`. No non-null assertion, no required JSON field,
no uncaught throw. Every JSON-shape divergence in this crate is on an endpoint the
explorer never calls.

### Failure-mode gaps (real status divergences, all non-explorer, all recoverable)

- `/v2/parcels/{x}/{y}`, `/v2/estates/{id}`, `/v2/contracts/.../tokens/{id}`: transient
  DB/subgraph errors surface as **404 instead of 500** (`.ok().flatten()`).
- All `*.png` renderers: render-error path returns **500 bare-string** instead of
  upstream `500 {ok:false,error}` JSON — effectively unreachable.

### Rejected / corrected

- No re-check finding rejected (list was empty); crate-level narrative is accurate.
- Prior parity report (`docs/parity/map.md`) items on missing dissolved-estate
  fallback, missing LAND immutable cache-control, JSON district-404 body, and the
  exclude-param divergence are **STALE** — all fixed on the committed tree (see the
  endpoint detail above).
