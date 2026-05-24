# lamb2 / TS-catalyst parity quirks

catalyrst aims for byte-level response parity with the TypeScript catalyst
(`content-server` + `lamb2`). Many handler decisions look arbitrary in
isolation but exist because the reference does the same thing; changing
them silently breaks clients that depend on the wire shape. This page
catalogs the load-bearing quirks endpoint-by-endpoint so a future
maintainer doesn't "simplify" them away.

If you change behavior on any endpoint listed here, **first** run
`catalyrst-conformance --baseline <public-peer> --candidate <local>` and
confirm the diff doesn't widen.

## Cross-cutting

### Error body shape (`crates/catalyrst-server/src/errors.rs`)

Verified against running lamb2 on `:5142`. Contract:

```json
{"error": "<Class>", "message": "<details>"}
```

| Status | `error` field (case is part of the contract) |
|--------|----------------------------------------------|
| 400    | `"Bad request"` — lowercase `r`              |
| 404    | `"Not Found"`                                |
| 500    | `"Internal Server Error"`                    |

- `message` is OPTIONAL — content-server errors via `AppError` pass
  `None` and so emit just `{"error": ...}` for back-compat.
- 500 responses MUST NOT leak the inner `Internal(String)` payload.

### `entityType` query parameter (`crates/catalyrst-server/src/query_params.rs`)

Mirrors reference `parseEntityType` byte-for-byte: **strip ONE trailing
`s`**, uppercase, look up. This produces the well-known quirk:

| Input        | Resolves to | Notes                                   |
|--------------|-------------|-----------------------------------------|
| `scene`      | `SCENE`     | identity                                |
| `scenes`     | `SCENE`     | trailing-s stripped                     |
| `profile`    | `PROFILE`   | identity                                |
| `profiles`   | `PROFILE`   | trailing-s stripped                     |
| `outfit`     | (unknown)   | strips to `OUTFI`, no enum match → 400  |
| `outfits`    | `OUTFIT`    | strips to `OUTFIT` → match              |
| `outfitss`   | `OUTFITS`   | the only way to get `OUTFITS`           |

This is reference-parity, not a bug. Outfits are intentionally rejected on
`/entities/{type}` and `/audit/{type}/{entityId}`; they're served only via
`/lambdas/outfits/{id}`.

### Pagination dialects

Three distinct pagination styles, codified by `query_params::PaginationPolicy`:

| Dialect          | pageNum         | pageSize          | Used by                                            |
|------------------|-----------------|-------------------|----------------------------------------------------|
| strict           | reject if <1    | reject if >max    | content-server (`/deployments`, etc.)              |
| pass-through     | reject if <1    | reject if >max; pass through zero/negative | user-items, explorer (`/lambdas/users/...` and `/lambdas/explorer/...`) |
| clamped          | clamp to 1      | clamp to max      | `/lambdas/users/.../lands`, `/.../lands-permissions` |

`pass-through` exists because the reference applies pageNum/pageSize via
JS `Array.slice` semantics at the use site — negative values yield
negative offsets that JS clamps to 0. Reproducing this in Rust requires
*not* validating up front.

All three dialects use **saturating arithmetic** for `(pageNum-1) * pageSize`
so a hostile `pageNum=i64::MAX` yields an empty page rather than wrapping.

### Address validation as a security gate

`is_valid_eth_address` is called BEFORE any DB / NFT-worker / subgraph
lookup whenever `address` is in the URL path. Lambdas like the
third-party-wearables path interpolate the address into outbound URLs;
validation prevents request-shaping.

### URN normalization between `:ethereum:` and `:mainnet:`

L1 (collections-v1) wearables are stored in the squid as
`urn:decentraland:mainnet:collections-v1:...` but every wire surface
(profiles, lambdas catalog, etc.) refers to them as
`urn:decentraland:ethereum:...`. catalyrst's SQL rewrites the network
token in both directions:

- **on output:** `:mainnet:` → `:ethereum:` (`replacen(.., 1)` — only
  the first occurrence)
- **on input/cursor:** `:ethereum:` → `:mainnet:` (e.g. `lambdas_catalog`
  `nextLastId` and collection filters)

Without these rewrites, every legitimately-owned L1 wearable is silently
dropped from results.

### Unicode collation (`crates/catalyrst-server/src/handlers/definitions.rs`)

JS `String.prototype.localeCompare`-equivalent ordering via DUCET with
shifted variable-weighting (spaces/punctuation are quasi-ignorable at
primary level). Implemented via `feruca` with a thread-local `Collator`.

Bytewise `cmp` puts every uppercase letter before all lowercase and
mis-weights spaces/punctuation, diverging from lamb2's output. lamb2 uses
this collation for all name/urn sorts.

### CORS (`crates/catalyrst-server/src/cors.rs`)

Hand-rolled, not `tower_http::CorsLayer`. The tower layer answers
preflight with 200, appends a broad `Vary`, and emits
`Allow-Credentials` on origin-less requests — all of which would diff
against a live peer. The hand-rolled middleware:

- Reflects request `Origin` instead of `*` when credentials are involved.
- Short-circuits preflight to **204**, not 200.
- Omits CORS headers entirely on origin-less requests.
- Allow-methods list ends with `OPTION` (no trailing `S`) — verbatim from
  the reference; do NOT "fix" the typo.
- HSTS is deliberately not emitted by the app; it's the TLS terminator's job.

No OPTIONS handlers are registered in the router; the middleware short-
circuits all preflights.

## Per-endpoint quirks

### `GET /about` (`handlers/about.rs`)

- **comms probe caching** — `/about` is polled by explorers and realm-list
  in tight loops. The probe (ws-connector `/status` + stats `/core-status`)
  is cached for **5 s** with a **2 s per-probe timeout**. The cache TTL
  prevents request-burst fanout; the timeout guarantees a stalled probe
  never hangs the `/about` request.
- **`content.healthy` semantics** — true iff sync state is EXACTLY
  `"Syncing"`. lamb2 about-handler.ts:71. Note that catalyrst's internal
  `PartiallySynced` is mapped to `"Syncing"` on the wire in
  `bin/live.rs` for client parity — the upstream enum is only
  `{Bootstrapping, Syncing}`.
- **`comms.healthy` gating** — ws-connector `/status` reachable AND
  stats `/core-status` reporting healthy; any timeout/failure → unhealthy.
- The bff stub mirrors lamb2's literal stub where comms-health lives in
  `comms.healthy`.

### `GET /content/status` (`handlers/status.rs`)

Must merge `contentCluster.getStatus()` with `synchronizationState`.
`lastSyncWithDAO` MUST always be present (default to 0 if never synced).

### `POST /content/entities/active` (`handlers/active_entities.rs`)

- Body must contain `pointers` OR `ids` (mutually exclusive); both missing → 400.
- Empty array → returns `[]` with 200 (NOT 400; lamb2 parity).

### `GET /content/entities/{type}` and `/audit/{type}/{entityId}`

- `scenes` → `scene` canonicalization at the handler entry.
- `outfit` / `outfits` are intentionally REJECTED — only served via
  lambdas + the `/deployments?entityTypes=outfits` filter.
- Unknown `entityType` → 400.

### `GET /content/contents/{hashId}`

- No hash-format validation — handler does the lookup and 404s on miss.
- `Content-Length`, `ETag` headers are part of the contract.
- HEAD parity: same status + headers, empty body.

### `GET /content/contents/{hashId}/active-entities`

No hash-format validation — looks up and 404s on miss.

### `GET /content/available-content?cid=...`

- No hash-format validation — anything missing OR malformed → `available: false`.
- Missing `?cid=` query → 400.

### `GET /content/deployments`

- `entityType` canonicalization (see cross-cutting).
- `next` cursor in `pagination` is relative, query-only; the resolver in
  `catalyrst-sync` carries the path forward (`/pointer-changes` has the
  same issue — see "URL resolution" below).
- Response uses `{key, hash}` for content entries (NOT `{file, hash}` —
  that's the entity route's shape).

### `GET /content/pointer-changes`

- `entityType` canonicalization.
- **DESC keyset cursor**: `(ts < to) OR (ts = to AND LOWER(id) < LOWER(lastId))`
  — without the boundary exclusion, every delta sharing the boundary
  timestamp is re-served on the next page because OFFSET resets to 0.
- `authChain` always emitted; `includeAuthChain=true` only widens the SQL join.
- `entityTypes` field always emitted (even `[]`).
- **URL resolution for `next`:** upstream catalysts return relative,
  query-only `next` URLs (`"?from=&to=&limit=&lastId="`). They must be
  resolved against the *current* request URL (which still has
  `/pointer-changes` in the path), not against the bare server base —
  `url::Url::join` against just `server` drops the path and 404s.
  Regression test in `catalyrst-sync::pointer_changes`:
  `test_resolve_url_query_only_keeps_path`.

### `GET /content/snapshots`

State-dependent across servers; not a useful parity target unless both
peers have synced past the same point. See `docs/snapshot-generation.md`
for the CID-convergency invariants.

### `GET /content/queries/items/{pointer}/thumbnail` and `/image`

**Always sniff MIME from magic bytes**, unlike `/contents/:hash` which
only sniffs with `?includeMimeType`. Without this, PNG thumbnails come
back as `application/octet-stream`.

### `GET /content/queries/erc721/{chainId}/{contract}/{option}[/{emission}]`

- chainId → URN protocol mapping (the deprecated chains are kept for
  back-compat):

  | chainId   | protocol         |
  |-----------|------------------|
  | 1         | `ethereum`       |
  | 137       | `matic`          |
  | 80001     | `mumbai` (deprecated)   |
  | 80002     | `amoy`           |
  | 11155111  | `sepolia`        |
  | 5         | `goerli` (deprecated)   |
  | 4         | `rinkeby` (deprecated)  |
  | 3         | `ropsten` (deprecated)  |
- v1/v2 contract heuristic: starts with `0x` → v2; otherwise v1.
- Rarity-required guard: returns 400 "Wearable is not standard" when
  metadata has no `rarity`.

## /lambdas/*

### `POST /lambdas/profiles`

- Missing `ids` → 400.
- Empty `ids: []` → `[]` (200; NOT 400).

### `GET /lambdas/profiles/{id}` (and `/lambdas/profile/{id}` alias)

- Returns hydrated profile snapshots; `snapshots.{face256,body}` URLs are
  ALWAYS rebuilt as CDN URLs from entity id regardless of stored value
  (reference parity — do not echo stored URLs).
- Two-step ownership check: per urn, exact match OR any stored urn starts
  with `<u>:`. Batched in one SQL pair (exact + prefix); exact-match
  short-circuits prefix.
- URN normalization (`:ethereum:` ↔ `:mainnet:`) — see cross-cutting.
- Base wearables, base emotes, and emote urns without `:` skip ownership
  (everyone owns base; legacy short ids pass through).
- Pointers starting with `default` bypass ownership entirely.
- Prefix LIKE is emulated as `left(n.urn, length(p)) = p` — avoids
  LIKE-metachar hazards; the appended `:` anchors the delimiter so
  `0xabc` does NOT match `0xabcdef`.

### `GET /lambdas/users/{address}/wearables` and `/emotes`

- `pageSize` cap = 1000.
- Hostile `pageNum` saturates to empty page (see Pagination dialects).
- `includeEntities=true` AND `includeDefinitions=true` together → 400
  (mutually exclusive).
- `definition` field is the HYDRATED wearable/emote (image/thumbnail
  rewritten to content URLs, representations expanded to `[{key, url}]`),
  NOT raw metadata.
- URN normalization (`:mainnet:` → `:ethereum:` on output).
- Address validated BEFORE any external IO (security gate).

### `GET /lambdas/users/{address}/third-party-wearables[/{collectionId}]`

- `collectionId` shape: `urn:decentraland:<chain>:collections-thirdparty:<name>`
  (exactly 5 parts, non-empty name; reference `parseUrn` + type check). Else 400.
- Only third-party providers declaring ≥1 contract are considered.
- Provider id segment 4 is the `name`.
- Pagination cap is `i64::MAX` (lamb2 uses `Number.MAX_VALUE`).
- `createBaseSorting` only accepts `name ASC/DESC`.
- Stable ordering: lamb2's order is driven by content-server iteration
  order; we sort by name then urn to produce the same SET in stable
  order. (Element set is identical regardless.)

### `GET /lambdas/users/{address}/lands`, `lands-permissions`

- `pageSize` defaults to **100** when absent (NOT unbounded — verified
  against reference). Cap is 1000.
- `lands-permissions` queries the LAND subgraph directly because the
  updateOperator relationship lives only there (lamb2
  `fetch-permissions.fetchAllPermissions`).
- subgraph errors → 500.
- `parcel_permissions` rejects non-0x-40-hex addresses with 400.

### `GET /lambdas/users/{address}/names`

- `MAX_PAGE_SIZE = 1000`.

### `GET /lambdas/names/{name}/owner`

- 404 has **EMPTY BODY** (no JSON) when name isn't owned — lamb2 quirk.
- Strips trailing `.dcl.eth` before matching.
- Match is exact case-insensitive (`name_starts_with_nocase +
  name_ends_with_nocase` in the subgraph query).

### `GET /lambdas/parcels/{x}/{y}/operators`

Returns `None` when neither parcel nor estate exists at (x, y).
lamb2 throws `ParcelOrStateNotFoundError`; catalyrst returns null.
(Discrepancy with lamb2 — verify if this is intentional.)

### `GET /lambdas/explorer/{address}/wearables` and `/emotes`

- `itemType` field present only on wearables endpoint; emotes never have it.
- base-wearable + third-party collection types valid only on wearables;
  emotes only accepts `on-chain`.
- Sort/direction whitelist: only rarity/name/date × ASC/DESC.
- `rarestOptional` ordering: items WITH rarity sort before items WITHOUT;
  among rated, rarer first; rarity-less among themselves by urn.
- `newestOptional` / `oldestOptional`: dated items sort before dateless;
  `oldest` reverses the urn tiebreak.

### `GET /lambdas/collections/wearables` and `/emotes` (catalog)

- Wearables LIKE includes BOTH `'wearable%'` AND `'smart_wearable%'` —
  the plain `'wearable%'` filter misses smart wearables.
- `nextLastId` cursor is emitted in `:ethereum:` (L1) form but the squid
  stores `:mainnet:`. De-normalize the cursor before comparing or L1
  pages stall/repeat.
- Filter URNs (collection/item) also need `:ethereum:` → `:mainnet:` rewrite.
- Result URNs rewritten `:mainnet:` → `:ethereum:` before content-store
  lookup; without this every L1 match is silently dropped.
- Base wearables (off-chain) are prepended verbatim by lamb2's
  `paginateCatalogResults` — NOT re-sorted against on-chain.
- Pagination shape: omit `lastId` entirely; omit `next` when no next page.

### `GET /lambdas/nfts/collections`

Must query BOTH eth and matic subgraphs in parallel and keep
`[base, ...l1, ...l2]` order. 1000-row cap per subgraph is part of the
contract.

### `GET /lambdas/outfits/{id}`

- `namesForExtraSlots` is recomputed at serve-time = ALL owned names
  (full list).
- Extra-slot outfits (slot > 4) kept only when address owns ≥1 name;
  slots ≤ 4 always kept.
- lamb2's graph-backed wearable-ownership pruning inside each outfit is
  not implemented; stored wearables are kept.

### `GET /lambdas/contracts/{servers,pois,denylisted-names}` + `/third-party-integrations`

- 6h cache TTL on all four (these change rarely).
- **Stale-on-error** — on fetch failure, serve previous cache; only
  propagate if no cache exists.
- `contracts/servers`: skip `http://` (insecure); prepend `https://` if
  scheme missing.

### `GET /lambdas/contracts/servers` keccak/EIP-55

Self-contained keccak-256 + EIP-55 checksum lives inline so the `owner`
field matches the checksummed form lamb2 emits via eth-connect. uint256
is truncated to the low 64 bits — counts never exceed this.

### `GET /content/entities/active/collections/{collectionUrn}` (`filter_by_urn`)

URN shape whitelist: only `blockchain-collection-{v1,v2}`,
`blockchain-collection-third-party-name` (5 parts, non-empty name), or
off-chain `base-avatars` / `base-emotes` (with item). Everything else 400s.
Reproduces reference `parseUrn` + type filter.

### `POST /content/entities` (write path — `create_entity_multipart`)

Bootstrap guard: deployments are not allowed while catalyst is
bootstrapping (returns 503). The actual validation pipeline (auth chain,
entity structure, content hashes, rate limits, etc.) lives in
`crates/catalyrst-server/src/write_deployer.rs` and is documented in
[docs/write-path.md](./write-path.md).

## Out-of-scope (not in this doc)

- Sync orchestration parity → [docs/sync-pipeline.md](./sync-pipeline.md)
- Snapshot CID parity → [docs/snapshot-generation.md](./snapshot-generation.md)
- Third-party Merkle byte rules → [docs/third-party-merkle.md](./third-party-merkle.md)
- Auth-chain / EIP-1654 validation → [docs/auth-chain.md](./auth-chain.md)
- Rate-limiter math → [docs/write-path.md](./write-path.md)
