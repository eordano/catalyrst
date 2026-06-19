# Parity report — catalyrst-server (`content` lane)

Adversarial verification of the catalyrst-server (Rust port of catalyst content-server
+ lamb2 lambdas) against upstream `decentraland/catalyst` (content) and
`decentraland/lamb2` (lambdas). Each flagged finding was re-checked by reading both
the Rust and TS sources, and cross-referenced against what the Unity explorer client
actually consumes (`the Unity net-catalog` + `unity-explorer` source).

Method note: our live server (`catalyrst-live`, :5141) was not running and has no built
binary, so this is a static A/B (serde structs + `json!` literals + SQL vs upstream TS
source), plus live upstream reference payloads from content :5140 and lamb2 :5142.

## Per-endpoint table (flagged + reverified)

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /about | divergent | **same** (was "better" — rejected) | minor | comms always present (ours) vs only-when-ARCHIPELAGO_URL (upstream); adapter `offline:offline` vs `archipelago:...`; acceptingUsers ignores resource/MAX_USERS gating. All explorer-benign (see below). |
| GET /entities/{type} | match | better (≈same) | none | Both 1 SQL on miss + in-mem cache. Structurally equal; "better" rests on no-ORM only. |
| GET /entities/active/collections/{urn} | match | **worse** (confirmed) | minor | Upstream 24h-TTL LRU of prefix→ids; ours re-runs prefix SQL per page. Explorer never calls this endpoint. |
| GET/HEAD /contents/{hashId} | match | better (confirmed) | none | Optional X-Accel-Redirect zero-copy vs upstream node streaming. 0 SQL both. |
| GET /audit/{type}/{entityId} | match | better (confirmed) | none | Dedicated single-entity audit query vs upstream generic getDeployments+joins. |
| GET /deployments | match | better (confirmed) | none | 5s response cache + keyset; upstream serializes via sequentialExecutor, no response cache. |
| GET /pointer-changes | match | better (confirmed) | none | Keyset, no sequentialExecutor serialization; upstream serializes. |
| GET/HEAD /queries/items/{pointer}/thumbnail | match | better (confirmed) | none | 1 cached SQL + optional X-Accel zero-copy. |
| GET/HEAD /queries/items/{pointer}/image | match | better (confirmed) | none | Same as thumbnail. |
| GET /queries/erc721/{chainId}/{contract}/{option} | divergent | **same** (was "better" — rejected) | minor | Category-trait omission real on wire; explorer never calls erc721. Both 1 cached lookup. |
| POST /lambdas/profiles | match | better (confirmed) | none | 2 batched in-process SQL ownership vs upstream theGraph subgraph + redis. |
| GET /lambdas/profiles/{id} | match | better (confirmed) | none | 30s single-flight cache + 1 SQL + batched ownership SQL vs theGraph/redis. |
| GET /lambdas/collections/wearables | divergent | better (confirmed) | minor | pagination omits `lastId` echo; squid SQL vs theGraph subgraph. Explorer doesn't call. |
| GET /lambdas/collections/emotes | divergent | better (confirmed) | minor | Same `lastId` omission; squid SQL vs theGraph. |
| GET /lambdas/collections/wearables-by-owner/{owner} | match | better (confirmed) | none | Grouped squid-nft SQL vs theGraph + redis. |
| GET /lambdas/collections/emotes-by-owner/{owner} | match | better (confirmed) | none | Same grouped SQL. |
| GET /lambdas/contracts/servers | match | **worse** (confirmed) | none | N+1 sequential eth_calls cold; 6h cache mitigates. |
| GET /lambdas/contracts/pois | match | **worse** (confirmed) | none | N+1 sequential eth_calls cold; 6h cache. |
| GET /lambdas/contracts/denylisted-names | match | **worse** (confirmed) | none | N+1 sequential eth_calls cold; 6h cache. |
| GET /lambdas/users/{address}/wearables | match | better (confirmed) | none | 1 grouped SQL + in-mem slice vs theGraph + redis + content HTTP. |
| GET /lambdas/users/{address}/emotes | match | better (confirmed) | none | Same single grouped SQL path. |
| GET /lambdas/users/{address}/third-party-wearables | match | **worse** (confirmed) | minor | Per-provider uncached prefix-scan loop; upstream collectionsCache + cache-warmer. |
| GET /lambdas/users/{address}/third-party-wearables/{collectionId} | match | **worse** (confirmed) | minor | Same per-provider prefix loop, one provider; upstream cached. |
| GET /lambdas/users/{address}/names | match | better (confirmed) | none | 2 squid SQL + DB LIMIT/OFFSET vs names subgraph + redis. |
| GET /lambdas/users/{address}/lands | match | better (confirmed) | none | DB LIMIT/OFFSET vs upstream fetch-all-then-slice-in-memory. |
| GET /lambdas/names/{name}/owner | match | better (confirmed) | none | 1 indexed SQL vs names subgraph + redis. |
| GET /lambdas/explorer/{address}/wearables | match | better (confirmed) | none | explorer_cache single-flight + in-process SQL vs theGraph+redis+content HTTP. |
| GET /lambdas/explorer/{address}/emotes | match | better (confirmed) | none | Same. |
| GET /lambdas/outfits/{id} | match | better (confirmed) | none | 60s single-flight cache + 2 SQL vs content fetcher + namesFetcher/alchemy + redis. |

## Confirmed shape issues

All three flagged shape divergences are REAL on the wire. None breaks the Unity explorer
(verified against `About.gen.cs`, `RealmController.cs`, and the net-catalog).

1. **GET /about — comms object always present + adapter value (minor).**
   - Ours (`crates/catalyrst-server/src/handlers/about.rs:366`) emits `comms: Some(comms)`
     unconditionally, with `adapter = "offline:offline"` from the `COMMS_FIXED_ADAPTER`
     env default. Upstream (`lamb2/.../about-handler.ts:77-93`) emits `comms` ONLY when
     `ARCHIPELAGO_URL` is set, and uses `adapter = archipelago:archipelago:ws://.../archipelago/ws`.
     Live lamb2 :5142 here has ARCHIPELAGO_URL unset → omits `comms` entirely.
   - **Explorer impact: NONE.** `RealmController.cs:440` resolves the adapter as
     `about.comms?.adapter ?? about.comms?.fixedAdapter ?? "offline:offline"` — a missing
     `comms` object null-coalesces to the exact same `"offline:offline"` that ours emits
     explicitly. `ForkGlobalRealmRoom.cs:43-44` then treats `offline:offline` as a
     first-class value (returns the Null comms room). So upstream-absent and ours-present
     converge to identical client behavior.

2. **GET /about — acceptingUsers gating (minor).**
   - Ours (`about.rs:322`) sets `accepting_users = healthy`. Upstream
     (`about-handler.ts:74,82`) factors `!resourcesStatusCheck.areResourcesOverloaded()`
     and `(!MAX_USERS || userCount < MAX_USERS)`.
   - **Explorer impact: NONE.** `acceptingUsers` exists in `About.gen.cs` (generated
     protobuf getter/setter only) but has no consumer read anywhere in the explorer
     gameplay code. The realm never gates the client on it.

3. **GET /queries/erc721/... — Category trait omission (minor).**
   - When item metadata has no `category`, ours (`catalyrst-validator/src/erc721.rs:87-93`)
     omits the whole `{trait_type:"Category"}` object. Upstream
     (`catalyst/.../erc721/component.ts:98-101`) always pushes the object; with an
     undefined value JSON drops only the `value` key, keeping `{trait_type:"Category"}`.
   - **Explorer impact: NONE.** The Unity client never calls `/queries/erc721/...`
     (net-catalog has zero erc721 endpoints; it is an OpenSea-style NFT-metadata route for
     marketplaces/wallets). Also only triggers for non-standard items missing a required
     schema field.

4. **GET /lambdas/collections/{wearables,emotes} — pagination omits `lastId` echo (minor).**
   - Ours (`handlers/lambdas_catalog.rs:418-421,478-481`) builds pagination with only
     `limit` + optional `next`. Upstream (`wearables-catalog-handler.ts:69-73`) emits
     `{ limit, lastId, next }`, echoing the request cursor when present.
   - **Explorer impact: NONE.** The explorer never calls the catalog endpoints, and no
     client reads `pagination.lastId` (the cursor is carried in the opaque `next` query
     string, which ours preserves). Pure echo field.

## Confirmed efficiency wins (with structural reason)

These "better" verdicts survived verification because the structural reason is real (not a
language/runtime argument):

- **Content blob serving (/contents, queries items image/thumbnail):** optional
  `X-Accel-Redirect` hands the file to nginx `sendfile()`, bypassing userspace buffering;
  upstream always streams through node `content.asRawStream()`.
- **/deployments:** 5s-TTL response cache keyed on normalized query (x-cache HIT/MISS) +
  keyset pagination; upstream has no response cache and serializes every call through
  `sequentialExecutor.run` (one at a time). Confirmed in upstream source.
- **/pointer-changes:** keyset query with no sequentialExecutor serialization bottleneck;
  upstream serializes.
- **/audit:** dedicated single-row audit query vs upstream reusing the generic
  getDeployments path (full deployments query + content joins, then `[0]`).
- **Ownership/elements family (profiles, profiles/{id}, *-by-owner, users/{wearables,
  emotes,names,lands}, explorer/{wearables,emotes}, outfits):** in-process indexed Postgres
  (squid_marketplace.*) replaces upstream's remote theGraph subgraph queries + redis
  ownership caches + content HTTP fetches. Verified concretely:
  - profiles ownership = exactly 2 batched SQL (`resolve_ownership_batch`: one `urn = ANY($2)`
    exact query + one prefix EXISTS query) vs upstream `ownedNFTsByAddress` →
    `querySubgraphByFragments` over theGraph (`logic/ownership.ts`).
  - lands: upstream `fetchAndPaginate` (`logic/pagination.ts:36`) fetches the FULL owned set
    from the land subgraph then `.slice()` in memory; ours pages with DB LIMIT/OFFSET.
  - many of these also add a 30s/60s single-flight response cache (profiles_batch_cache,
    profile_cache, explorer_cache, outfits_cache) absent upstream.

## Confirmed efficiency LOSSES (we are worse)

- **/entities/active/collections/{urn} (minor):** upstream caches the prefix→entityIds list
  in a 24h-TTL LRU (`active-entities/component.ts:36-48`, `withPrefix` lines 283-294) so
  repeated paging over a collection issues 0 prefix queries. Ours
  (`bin/live.rs:812-847`) re-runs `get_item_entities_ids_matching_collection_urn_prefix`
  SQL on every page, then slices in Rust. Indexed query, but re-scanned per page.
- **/lambdas/users/{address}/third-party-wearables[/{collectionId}] (minor):** ours
  (`lambdas_user_items.rs:fetch_collection_entities`) runs an uncached paged
  `active_entities_by_prefix` loop per provider on every request. Upstream
  `entities-fetcher.ts:107-129` has a `collectionsCache` keyed by collectionId (plus a
  `third-party-collections-cache-warmer`) — warm hits do zero prefix/pagination queries,
  only filtering cached refs by owned NFTs. This is a genuine structural disadvantage.
- **/lambdas/contracts/{servers,pois,denylisted-names} (none):** cold path is N+1
  SEQUENTIAL `eth_call` RPCs (count, then per-index id/record) in a `for ... .await?` loop
  (`lambdas_contracts.rs:325-339`), with hand-rolled keccak/ABI/EIP-55. Mitigated by a 6h
  process-wide cache + stale-on-error, so severity none. Both sides cache 6h; our cold path
  is the heavier pattern.

## Rejected during verification

- **GET /about efficiency "better" — REJECTED, downgraded to "same".** The finding claimed
  upstream "re-probes content+lambdas+archipelago status URLs per request with no
  coalescing." False: upstream `status.ts` wraps every probe in an `LRUCache` with `ttl:
  60s` and an async `fetchMethod`, and `lru-cache.fetch` coalesces concurrent fetches for
  the same key (single-flight). Upstream therefore caches AND coalesces just like ours
  (ours uses a 5s TTL + AtomicBool single-flight). Not structurally better; if anything our
  shorter TTL re-probes more often.
- **GET /queries/erc721 efficiency "better" — REJECTED, downgraded to "same".** Both do
  exactly 1 cached entity lookup (`find_entity_by_pointer` → cached
  `active_entities_by_pointers`; upstream activeEntities cache). The only remaining basis
  for "better" was "pure-Rust formatting, no ORM" — a language/runtime argument, which the
  skeptic rules exclude.
- **GET /entities/{type} efficiency "better" — flagged as marginal.** Both do 1 SQL on miss
  + an in-memory cache (ours RwLock map, upstream LRU). The finding itself concedes "roughly
  same structural cost"; the "better" label rests on avoiding ORM hydration. Kept as
  better-but-effectively-same; no independent structural advantage.
