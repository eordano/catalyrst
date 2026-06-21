# Parity report — catalyrst-market (service "market")

Upstream baseline: `decentraland/marketplace-server` (TS).
Our port: `crates/catalyrst-market`.
Verified statically against upstream source (marketplace-server is not running locally);
client-impact cross-checked against the real consumer (`decentraland/marketplace` webapp,
which routes all marketplace-server calls through `decentraland-dapps` `BaseAPI`) and the
Unity net-catalog.

## Who actually calls this API

The Unity explorer does **not** call marketplace-server REST endpoints directly — the
net-catalog (`the Unity net-catalog`) shows the client only
*links* to the marketplace web app (`market.decentraland.{ENV}`). The real consumer is the
**marketplace webapp**, whose `BaseAPI.request` normalizes responses via `parseResponse`:

- If the body has a boolean `ok` field, it is returned as-is and `request` resolves to `body.data`.
- If the body has no `ok` field, `parseResponse` wraps it as `{ok:true, data:<body>, error:''}`,
  so `request` resolves to the **whole body**.

This normalization is load-bearing for judging the "missing ok" / "wrapper" findings below.

## Per-endpoint table (flagged + corrected only; all other rows from the input verdict set are unchanged "match"/"same")

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /v1/catalog | divergent | better | minor | picks always 0; ours also never emits `picks:null` for unpicked items (upstream does). Saves serial picks round-trip. |
| GET /v2/catalog | divergent | better | minor | Same as v1, with-trades query variant. |
| GET /v1/nfts | divergent | worse (not a real win) | major | order/rental + activeOrderId/openRentalId always null — on-sale NFTs lose price/order data. |
| GET /v1/items | unknown (stub) | worse (non-functional) | breaks-client | `get_items` returns `(vec![],0)` unconditionally; shape unverifiable. |
| GET /v1/orders | divergent | better | **major (upgraded)** | Not just a `.0` float quirk: createdAt/updatedAt are emitted in **milliseconds** (×1000) vs upstream **seconds** — a 1000× value divergence. |
| GET /v1/bids | divergent (cosmetic) | same | **minor (downgraded)** | Wrapper diff is invisible after BaseAPI normalization — see rejection. |
| GET /v1/sales | match | same | none | Confirmed (input verdict self-corrected better→same). |
| GET /v1/trendings | divergent | worse | major | Missing Cache-Control header; data always [] because the (fully-ported) trending logic feeds off the items stub. |
| GET /v1/stats/{category}/{stat} | divergent | worse | minor | Missing Cache-Control + etag + Last-Modified + content-length headers. |
| GET /v1/trades | divergent | worse (non-functional) | breaks-client | Missing `ok`; AND a stub (`get_trades` returns empty). After BaseAPI: ours `{data:{data,count}}` vs upstream `{data,count}` — extra nesting. |
| GET /v1/trades/{hashed_signature}/accept | divergent | same | major | Missing `ok`; after BaseAPI ours resolves to `{data:Event}` vs upstream `Event`. NOT a stub — queries `marketplace.trades`, degrades to 404 if table absent. |

## Confirmed shape issues

### GET /v1/nfts — order/rental/activeOrderId always null (MAJOR, confirmed)
`ports/nfts.rs`: `get_nfts` maps every row to `NftResult { nft, order: None, rental: None }`
and `from_db_nft_to_nft` hard-codes `active_order_id: None` and `open_rental_id: None`
(lines 381-382, 809, 821). The `_caller` param is unused. Upstream
`ports/nfts/component.ts` runs a second `getOrdersQuery({nftIds, status: OPEN})`, fetches
rental listings via `rentals.getRentalsListingsOfNFTs`, then `fromNFTsAndOrdersToNFTsResult`
attaches the matching order/rental and sets activeOrderId. Real, client-affecting: on-sale
NFTs returned by ours carry no price/order — the marketplace asset view cannot show "Buy".

### GET /v1/orders — createdAt/updatedAt unit mismatch (UPGRADED to MAJOR)
The input finding flagged only the serde `f64` trailing-`.0` representation
(`1700000000000.0` vs `1700000000000`). That is real (serde_json/ryu emits `.0` for
integral f64; fields are `f64` at `ports/orders.rs:52-56`), but the dominant divergence is
**units**. The squid `squid_marketplace."order"` table stores:
- `created_at` / `updated_at` as epoch **seconds** (e.g. `1580740547`)
- `expires_at` as epoch **milliseconds** (e.g. `1583334000000`)
(verified by querying the live squid DB column types + sample rows.)

Upstream `getLegacyOrdersQuery` selects `ord.created_at`, `ord.updated_at`, `ord.expires_at`
**raw** → emits createdAt/updatedAt in seconds, expiresAt in ms. Ours multiplies
created_at/updated_at by `1000.0` (`ports/orders.rs:224,228`) → milliseconds, while leaving
expires_at raw. Net effect: ours' createdAt/updatedAt are 1000× upstream's. `@dcl/schemas`
types these as plain `number`; any client comparing/sorting/formatting order timestamps
against upstream values will be off by three orders of magnitude.

### GET /v1/catalog & /v2/catalog — picks always 0, and never null (MINOR, confirmed + refined)
`ports/catalog.rs:426-430` hard-codes `PickStats { count: 0, item_id, picked_by_user: None }`
for every item. Upstream `ports/catalog/component.ts` runs `picks.getPicksStats(...)` then
`enhanceItemsWithPicksStats`, which sets `picks: stats ?? null` — i.e. real counts when
picked, and `picks: null` when not. Ours always emits a zero-count `picks` object and never
`null`. Two divergences: (1) values always 0; (2) shape is always an object, never null.
Field keys present on both; affects favorite/pick UI counts only.

### GET /v1/trendings — missing Cache-Control + empty data (MAJOR, confirmed)
`handlers/trendings.rs` sets no response headers; upstream `trending-handler.ts` emits
`Cache-Control: public,max-age=3600,s-maxage=3600`. The trending selection logic in
`ports/trendings.rs` is a faithful port (real `sale` query, sales/volume cuts), but it
looks items up via `ItemsComponent::get_items`, which is the stub returning empty → the
`item_index` is always empty → `out` is always `[]`. So both the header and the data
diverge.

### GET /v1/stats/{category}/{stat} — missing caching/conditional headers (MINOR, confirmed)
`handlers/stats.rs` sets no headers; upstream `stats-handler.ts` sets `Cache-Control`
(1h s-maxage), `Last-Modified`, `etag` (computed from `JSON.stringify(data)`) and
`content-length`. Body JSON shape matches; only CDN/conditional-request posture diverges.

### GET /v1/items & GET /v1/trades — stubs (BREAKS-CLIENT, confirmed)
- `ports/items.rs:281-290`: `get_items` returns `(Vec::new(), 0)` unconditionally
  (depends on un-ported `marketplace.mv_trades`). Envelope `{data,total}` matches upstream
  (bare, no `ok`), but data is always empty so item field shape is unverifiable at runtime.
- `ports/trades.rs:79-86`: `get_trades` returns `(Vec::new(), 0)` unconditionally.
  Additionally missing the top-level `ok` (see below).

### GET /v1/trades — missing `ok` (BREAKS-CLIENT, confirmed)
`handlers/trades.rs:22-27` returns `{data:{data,count}}`. Upstream `trades-handler.ts`
returns `{ok:true, data:{data,count}}`. After BaseAPI normalization a consumer expecting
`request(...)` to be `{data,count}` gets `{data:{data,count}}` from ours (extra nesting) —
a real shape difference. (No clear marketplace-webapp consumer of the unfiltered list
endpoint was found; it appears admin/debug, hence the divergence is real but its blast
radius on the webapp is limited.)

### GET /v1/trades/{hashed_signature}/accept — missing `ok` (MAJOR, confirmed)
`handlers/trades.rs:43-62` returns `{data:<event json>}`; upstream returns
`{ok:true, data:<Event|null>}`. After BaseAPI normalization ours resolves to
`{data:Event}` whereas upstream resolves to `Event` — divergent. Correction to the input
finding: this is **not** a stub — `ports/trades.rs:158-219` queries `marketplace.trades`
by hashed_signature, 404s if not found, and gracefully degrades to 404 if the table is
absent. The Event is synthesized in code from the head trade row.

## Confirmed efficiency wins (with structural reason)

- **GET /v1/orders — 1 query vs 2.** Ours uses a single SELECT with `COUNT(*) OVER() AS count`
  (`ports/orders.rs:147`). Upstream `ports/orders/component.ts:11-13` runs
  `Promise.all([getOrdersQuery, getOrdersCountQuery])` = two separate statements. One fewer
  round-trip + one fewer planner pass on the same pool. Structural, not language-based.
- **GET /v1/catalog & /v2/catalog — saves the serial picks round-trip.** Ours runs items +
  count concurrently via `tokio::try_join!` (`ports/catalog.rs:1569-1571`) and never queries
  picks. Upstream runs items+count via `Promise.all` and then a **serial** third
  `picks.getPicksStats` query (`ports/catalog/component.ts:48-85`), plus `analytics.track`
  when `search` is set. The win is real but is bought with the picks=0 correctness loss
  above — it is doing strictly less work.

## Confirmed efficiency "worse" / non-wins

- **GET /v1/nfts** — ours is 1 query vs upstream 1-2, but only because it skips the
  order/rental enrichment entirely. Not a real win (confirmed; matches input verdict).
- **GET /v1/items, /v1/trades, /v1/trendings** — 0 (or near-0) useful queries because the
  endpoints are stubs / feed off a stub. Non-functional, not faster (confirmed).
- **GET /v1/stats, /v1/trendings** — absent CDN/cache headers mean every request hits origin
  vs upstream's 1h s-maxage edge caching. Structurally worse caching posture (confirmed).
- **GET /v1/bids — efficiency same** (confirmed): both run a single query with `COUNT(*) OVER()`
  over a UNION of bid sources, no cache.

## Rejected during verification

- **GET /v1/bids "wrapper mismatch → breaks-client" — REJECTED (downgraded to minor/cosmetic).**
  The input claimed ours returns bare `{results,total,page,pages,limit}` while upstream returns
  `{ok:true, data:{results,...}}`, so `body.data.results` would be undefined. Verified the bare
  vs wrapped serialization is real (`catalyrst-types::PaginatedResponse` serializes with no `ok`;
  upstream `bids-handler.ts` wraps in `{ok:true,data:{...}}`). BUT the real consumer
  (`marketplace` webapp `BidAPI extends BaseAPI`) routes through `BaseAPI.request` →
  `parseResponse`: a body with no `ok` is wrapped and `request` returns the bare body; a body
  with `ok` returns `body.data`. **Both paths resolve to the identical inner
  `{results,total,page,pages,limit}` object.** So the wrapper diff is invisible to any
  BaseAPI-based client. The webapp saga then reads `response.results` (verified in
  `modules/bid/sagas.ts:166-167,183`), which is present in both. No client breakage. Severity
  reduced from breaks-client to minor (cosmetic for raw/curl consumers only). NOTE: this
  normalization is exactly why the /v1/trades and /accept "missing ok" findings still bite —
  there the inner `data` is itself an envelope (`{data,count}` / `Event`), so the wrap/unwrap
  asymmetry produces an extra nesting level rather than canceling out.

- **GET /v1/orders "minor float-roundtrip quirk" framing — REJECTED in favor of the units bug.**
  The `.0`-formatting observation is technically true but undersells the defect: the actual
  divergence is a 1000× unit mismatch on createdAt/updatedAt (ms vs s). Reclassified to MAJOR
  (see confirmed shape issues).
