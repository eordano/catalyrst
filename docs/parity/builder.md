# Parity report — catalyrst-builder (service "builder")

Upstream: `decentraland/builder-server`.
Crate: `crates/catalyrst-builder`.
Live diff: not applicable (builder-server not running locally; compared statically).

Verified against the Unity explorer's actual consumption path
(`unity-net-catalog` + `unity-explorer` source) so that "divergent" findings are
graded by whether the *explorer* is actually affected, not just whether the JSON
differs in the abstract.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| `GET /ping` | match | same | none | catalyrst-local health probe; no upstream counterpart. Static `pong`. |
| `GET /v1/collections/{id}/items` | match (for the explorer) | better | minor | Per-item `FullItem` fields match. Wrapper divergence exists only in paginated mode, which the explorer never triggers (see below). Ignored filter params likewise never sent. Efficiency win is structural (no subgraph/catalyst/committee round-trips) with an intra-DB N+1 caveat. |
| `GET /v1/storage/contents/{hash}` | match | same | none | 301 redirect, pure URL synthesis. Only diff is Cache-Control whitespace (semantically identical). |
| `POST /v1/newsletter` | match | worse | minor | Response body is identical (`{"ok":true}`) once you account for JSON dropping `undefined`. Real diffs are SaaS-side behavior + an extra local DB write. |

## Confirmed shape issues

### GET /v1/collections/{id}/items — per-item fields MATCH; wrapper diverges only in a mode the explorer never uses

Confirmed by reading both sides:

- Per-item `FullItem` object fields match one-for-one (`crates/catalyrst-builder/src/ports/items.rs:57-138`
  vs `FullItem = Omit<ItemAttributes,'urn_suffix'> + extras` in
  `builder-server/src/Item/Item.types.ts:54-66`). `local_content_hash` is present on
  both; only `urn_suffix` is omitted upstream and ours emits the derived `urn` instead. OK.
- **Wrapper divergence is real but explorer-irrelevant.** Upstream returns the
  `{total,limit,pages,page,results}` envelope (inner key `results`) *only* when both
  `page` AND `limit` are supplied
  (`Item.router.ts:424-426` -> `generatePaginatedResponse`, `Pagination/utils.ts:22-37`);
  otherwise it returns the bare `items` array. Ours always returns the bare
  `{ok,data:[...]}` array (`handlers/collections.rs:67-68`).
  The Unity explorer builds the request by replacing only the `[COL-ID]` placeholder and
  appends **no** `page`/`limit` query params
  (`ApplicationParametersWearablesProvider.cs:78`; pagination is done client-side via
  `Skip`/`Take`, lines 58-59/118-119). It then deserializes `data` as a
  `List<BuilderWearableMetadataDto>` (`WearableDTO.cs:64`) — i.e. it *requires* a bare
  array and would break on the envelope. So for the explorer, ours and upstream are
  byte-compatible. A hypothetical paginating client would diverge; the explorer does not.
- **Ignored filter params (`status`/`mappingStatus`/`synced`) are explorer-irrelevant.**
  Our SQL applies only `name` + `limit`/`offset` (`items.rs:181-202`); the `status`,
  `mapping_status`, `synced` fields are carried on `ItemQuery` but never bound. Upstream
  honors them (`Item.service.ts:230-237`). The explorer sends none of them, so no impact.
- **`isMappingComplete` only matters for third-party items**, which this crate does not
  model; absent for DCL collections on both sides. Non-issue for DCL flows.
- Semantic divergence is genuine but not a shape diff: our `is_published/is_approved/
  in_catalyst/total_supply/content_hash/catalyst_content_hash` come straight from DB
  columns, whereas upstream overwrites them from TheGraph + catalyst peer via
  `Bridge.consolidateItems`. Same JSON types; values can lag on-chain truth. This is a
  data-freshness tradeoff, not a contract break.

Net: no shape issue the explorer can observe. Graded match for the explorer; the
paginated-envelope and ignored-filter divergences are documented as latent
non-explorer concerns.

## Confirmed efficiency wins (structural)

### GET /v1/collections/{id}/items — "better" CONFIRMED (structural, not language)

Read both implementations:

- **Auth:** ours calls the synchronous `require_signer` -> `verify_auth_chain`
  (`auth_chain.rs:178`, crypto `verify.rs:9-15`/`17-24`) — pure in-process ECDSA recovery,
  no network. Upstream `withAuthentication` uses `@dcl/platform-crypto-middleware`'s
  `verify` wired with `fetcher: peerAPI.signatureFetcher`
  (`middleware/authentication.ts:78-83`), which hits the catalyst peer for EIP-1654
  contract-wallet chains. (Caveat: for plain-ECDSA chains the upstream verify is also local,
  so the auth-step win is conditional.)
- **Access control:** ours = 1 SQL (`collection_owner`) + in-memory admin allowlist
  (`collections.rs:44-55`, `items.rs:159-169`). Upstream = `Promise.all([Ownable.isOwnedBy
  (DB), isCommitteeMember, hasManagerAccess])` where `isCommitteeMember` calls
  `collectionAPI.fetchCommittee()` — a TheGraph subgraph HTTP call
  (`Collection/access.ts:23-25`, `Committee/utils.ts:9-11`). So ours avoids one off-box
  subgraph round-trip on every request.
- **Data merge:** upstream `getDCLCollectionItems` makes 2 off-box HTTP calls per request —
  `collectionAPI.fetchCollectionWithItemsByContractAddress` (subgraph) and
  `Bridge.consolidateItems` -> `peerAPI.fetchItems` (catalyst peer)
  (`Item.service.ts:354-373`, `Bridge.ts:265-284`). Ours makes ZERO external calls.

Net off-box round-trips per request: ours 0, upstream ~3 (auth peer for EIP-1654 +
committee subgraph + collection subgraph + catalyst peer). The win is structural —
it is the absence of subgraph/catalyst reconciliation, not Rust-vs-TS. No caching on
either side.

**N+1 caveat (intra-DB, the one place ours is worse):** ours runs 1 items query then 1
`item_contents` query *per row* (`items.rs:206-211`). Upstream returns items + total in a
single windowed query (`count(*) OVER() AS total_count`, `Item.model.ts:206`) with
`contents` carried as a jsonb column on the items row (migration
`1599064178474_add-contents-to-items`), so no per-item contents fetch. Our intra-DB
pattern is worse; our total cross-service cost is far lower. Net: better overall, with
the N+1 hurting only on large collections.

## Confirmed efficiency regressions

### POST /v1/newsletter — "worse" CONFIRMED (structural)

Ours does 1 SQL upsert into `newsletter_subscriptions` (`items.rs:294-305`,
`handlers/newsletter.rs:32`) **plus** the SaaS forward. Upstream does NO DB write at all —
forward-only (`Newsletter.router.ts` subscribe -> `Newsletter.model.ts:10-40`). So per
request ours incurs an extra DB round-trip upstream lacks. Both forward fire-and-forget
with swallowed errors. The extra write is a deliberate local-archive choice but is
structurally more work. Severity minor (single cheap upsert on a low-traffic path).

## Rejected during verification

- **REJECTED — newsletter response-shape divergence (`{ok:true}` vs `{ok:true,data:null}`).**
  The finding claimed upstream emits `data:null`. Verified false. decentraland-server's
  `handleRequest` calls `sendOk(data)` = `{ ok: true, data }` with the handler's return
  value; the subscribe handler returns `undefined`, and `JSON.stringify({ok:true,
  data:undefined})` drops the `undefined` key, yielding exactly `{"ok":true}` — identical
  to ours (verified against `decentraland-server/dist/server/index.js:12-54` and a live
  `node` check). No response-shape divergence exists; the body is a MATCH.

- **REJECTED as an explorer-affecting shape issue — collection-items "WRAPPER DIVERGENCE"
  severity.** The divergence is real in code but cannot affect the explorer: the explorer
  never sends `page`/`limit` to this endpoint and deserializes `data` as a bare `List<>`
  (`WearableDTO.cs:64`, `ApplicationParametersWearablesProvider.cs:78`). Kept as a
  documented latent concern for non-explorer clients, but downgraded from a client-visible
  shape break to a non-issue for our consumer; the endpoint is graded "match" for the
  explorer.

- **PARTIALLY QUALIFIED — auth efficiency win.** The "in-process AuthChain verify, zero
  network" claim is correct for our side, but upstream's auth is only off-box for EIP-1654
  contract-wallet chains; for plain-ECDSA chains both verify locally. The structural win at
  the auth step is therefore conditional. The dominant, unconditional wins are the data-merge
  subgraph + catalyst calls and the committee subgraph — those stand.
