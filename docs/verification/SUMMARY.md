# Catalyrst service-plane verification — authoritative summary

Branch `feat/service-plane-crates`, committed tree. Synthesizes the 23 per-service
re-checks under `docs/verification/*.md`. Every claim below was re-verified by reading
the crate source, matching response shapes against the Unity DTOs/converters, and
checking the Unity net-catalog (`the Unity net-catalog`)
to decide whether the explorer client actually calls the endpoint.

"Client-called" = present in the net-catalog and parsed by a C# converter.
Endpoints not in the net-catalog cannot break or crash the explorer regardless of
divergence; those divergences are recorded but scored `none`/`low`.

---

## 1. Totals

- **Services verified:** 23 crates
- **Endpoints reviewed:** 258
- **Confirmed issues:** 53

### Issues by severity

| Severity | Count | Meaning |
|---|---|---|
| breaks-client | 1 | client-called path returns a status/shape that aborts a core flow |
| major | 2 | client-called or federation-reachable, can fail a whole fetch / block startup |
| minor | ~33 | real divergence, client-tolerated or not client-called |
| low / cosmetic / none | ~17 | crawler-only, dApp-only, ops-only, or input-more-permissive |

### Headline counts (machine summary)

- `breaks_client` (client-called paths that abort a core flow): **2**
  — `content GET /about` (503 aborts realm change) and `archipelago GET /status`
  (404 blocks bootstrap).
- `client_crashes` (a Unity null-deref / uncaught engine crash): **0** on the
  committed tree. Every flagged "crash" resolves to either a *request-throws-but-caught*
  (handled error, not an engine crash) or a *latent, currently-unreachable* C# fragility.

---

## 2. CLIENT-CRASH / FLOW-BREAK risk list (the must-fix list)

No endpoint produces a true Unity engine null-crash today. The genuinely
client-affecting defects are the two flow-breaks plus a small set of
latent/federation-reachable hazards. Ordered by blast radius.

### A. Flow-breaks (client-called, abort a core flow) — MUST FIX

1. **`content GET /about` -> 503 on comms-down** (`about.rs:307-322,366`).
   `healthy = content_healthy && comms_healthy`, and `comms` is **always** emitted.
   Upstream lamb2 makes `comms` OPTIONAL and gates `healthy` only on `content && lambdas`.
   A down/unconfigured comms sidecar (`COMMS_WS_CONNECTOR_URL`/`COMMS_STATS_URL`,
   default `127.0.0.1:5001/5002`) -> `probe_comms()` `healthy=false` -> overall `false`
   -> **HTTP 503** where upstream returns 200. The 503 makes `WebRequestController.SendAsync`
   throw `UnityWebRequestException`, re-thrown as `RealmChangeException`
   (`RealmController.cs:148-149,196`) -> **realm entry aborts**. Request-throws / realm-break,
   not a null-deref. Severity **breaks-client**.

2. **`archipelago GET /status` not mounted on the explore bundle (5143)** — blocks
   client bootstrap. `api_router()` (`lib.rs:89-91`) omits `/status`; only the standalone
   binary mounts it. nginx (`docs/deploy/nginx-catalyrst-bundles.conf:111-118`) has no
   `/status` location and no catch-all -> realm host **404s**. Unity
   `MainSceneLoader.cs:520` health-probes `ArchipelagoStatus` via `MultipleURLHealthCheck`;
   `URLHealthCheck` treats 404 as failure (`ERROR_CODES={404,500}`), `ParallelHealthCheck`
   requires ALL urls -> the 404 fails the whole check across all 3 retries ->
   `IsLivekitDeadAsync` returns true -> LiveKit-down popup, **bootstrap stops**
   (`MainSceneLoader.cs:503-504`). Severity **major** (under-rated as "liveness semantics"
   in the original finding).

### B. Latent / federation-reachable crash vectors (not client-write-triggerable today)

3. **`communities GET /v1/communities/{id}/members` admin-role enum throw** —
   `community_members.role` is free VARCHAR (`migrations/0001_initial.sql:38`); the
   federation grant path (`fed/apply.rs:182-224` + `crl_apply_trg`,
   `migrations/0002_federation.sql:120-154`) persists `role='admin'`. The members port
   serializes it verbatim (`ports/members.rs:14`). Unity `CommunityMemberRole`
   (`CommunityMemberRole.cs:7-14`) has no `admin`; default Newtonsoft Populate
   (`CommunitiesDTOConverters.cs:73`) throws `JsonSerializationException`, failing the
   whole members fetch (no `SuppressToResultAsync` on the read leg,
   `CommunitiesDataProvider.cs:189-191`). **Not triggerable by client writes; triggerable
   by a replayed federated admin grant.** Severity **major** — the one substantive crash
   vector in this surface. FIX: add `Admin` to the C# enum or normalize the role on
   serialize.

### C. Latent C# fragilities that catalyrst output does NOT trigger (do not regress)

- **`media POST /translate` single-path `detectedLanguage` null-deref**
  (`DclTranslationProvider.cs:40-41`): unguarded. catalyrst-media **always** emits a
  populated `detectedLanguage` object (`backend/http.rs:74-84`, `backend/mock.rs`), so it
  cannot fire — but any future change that omits the field would crash the client.
- **`badges` zero-step divide path** (`BadgesUtils.cs:60`): `stepsDone*100/(nextStepsTarget ??
  totalStepsTarget)` div-by-zero only if a badge ships `totalStepsTarget=0` AND
  `nextStepsTarget=null`; data-dependent, not protocol.
- **`price GET /api/v3/simple/price`** C# Dictionary-indexer on empty/`{}` data — but the
  endpoint is hardcoded to CoinGecko and **never called** by Unity (mock surface only).
  Prior "major null-crash" verdict **rejected -> minor**.
- **`social-rpc GetBlockingStatus`** prior "major unguarded `PaginationData.Total`" crash —
  **rejected**: proto returns `{blocked_users, blocked_by_users}` (repeated strings, never
  null); C# reads exactly those. No crash path.

Everything else flagged as a crash collapses to the universal *request-throws-on-non-2xx*,
which the Unity `IWebRequestController` raises for every backend and every caller handles
as an error (toast/retry/degrade), not an engine crash.

---

## 3. Failure-mode matrix

How each service behaves under DB-down / bad-input / auth-failure, and where the error
*status* or *body* diverges from upstream. "Degrade" = serves a valid empty/default
response; "500-instead-of-degrade" = a transient backend blip fails a whole request that
upstream would still serve.

| Service | Panics at runtime? | DB-down behaviour | Bad-input | Error-status divergence | Error-body divergence |
|---|---|---|---|---|---|
| content | no (startup-only) | `/about` stub-only for content; comms probe gates 503 | n/a | **`/about` 503-vs-200 on comms-down** | content `{error}` vs lambdas `{message}` — mixed model |
| places | no | favorites/likes/report **503-vs-200** when writer unset; social-meta swallows to 200 | 400 on bad body (matches) | writer-missing 503 (upstream 200) | `federation_adr` extra field |
| events | no (DB is hard boot dep) | degrades: `total` fail-soft, categories fail-soft | n/a | sitemap **501 plain-text** (only non-envelope error) | sitemap text/plain vs `{ok:false,error}` |
| communities | no | (writes 501) | bans/requests **400-vs-401** on auth fail | **400-vs-401** auth; **200-empty-vs-404** no-existence-check | `{ok:false,message}` |
| comms | no | `?`->`Database`->500 (scrubbed) | malformed POST -> axum **422 plain-text** | scene-admin/bans **400-vs-401** auth | `{ok:false,message}` vs upstream `{message}`/`{error}` |
| archipelago | no | n/a | axum default rejections on POST | **`/status` 404** (not mounted) | extractor-reject body vs handler `{error}` |
| market | no | **500-instead-of-degrade**: DB-down 500 vs upstream catch->400 | bad `sortBy/network/status` **silently ignored -> 200** vs upstream **400** | `/v1/bids` flat vs `{ok,data}` wrapper; DB->500-vs-400 | bare-string vs `{ok}` on a few |
| explorer-api | no | synthetic About (no DB) | n/a | none material | none |
| ab-cdn | no | every fs/IO error -> 404 (degrade); `/health` 503 if out_root missing | n/a | none (S3-parity) | 404 text body (matches) |
| ab-registry | no | moka cache flush only | n/a | unset-token **401-vs-404** (benign) | message text differs |
| badges | no (startup-only) | request-throws on non-2xx (caught) | bool query defaults false (no 400) | none | none |
| builder | no (startup-only) | n/a | newsletter missing-email **422-vs-200**; **403-vs-401** | 422/403 minor | minor |
| camera-reel | no | n/a | n/a | `/api/docs` missing **404-vs-200** (cosmetic) | `{message}` (matches) |
| credits | no | captcha PNG raw bytes; DB-down mislabeled | wrong captcha **200 {ok:false}-vs-400** | minor | minor |
| economy | no (startup-only) | n/a | missing `transactionData` 400 has extra `code`; schema msg text | minor | extra `code` field |
| lists | no | missing table 42P01 -> **200 `{data:[]}`** (degrade) | n/a | none | none |
| map | no | **DB errors swallowed -> 404** on JSON meta (dApp-only) | n/a | transient-fail-as-404 (dApp viewers only) | none explorer-facing |
| media | no (DB is hard per-request dep) | **500-instead-of-degrade**: cache DB blip fails a translatable request | malformed body -> axum **plain-text 400/422** | 500 on translatable request vs upstream 200 | plain-text vs `{error}` |
| notifications | no | auth via `require_signer` | `PUT /subscription` **more permissive** (200 vs upstream 400) | input-only minor | minor |
| price | no | mount-wrapped; missing DB -> route 404 | no 400 for missing required params (vs CoinGecko 400) | no-400 + 500-empty-body + omit-vs-null | minor |
| rpc | no | WS relay; eth-gateway error -> null/empty result (silent) | JSON-RPC `-32700/-32600` | none material | none |
| social-rpc | no | **request-throws on DB error** across all read RPCs (RemoteError vs in-band) | missing user -> empty (vs upstream InvalidRequest) | wire-path differs, net client reaction same | none material |
| worlds | no (startup-only) | comms mint; `/contents` proxied | missing `pointers` -> **200 `[]`-vs-400** (parity-only) | **missing 503 capacity gate** on comms mint | `{error}` (matches) |

**No service panics on a request path.** All panics are startup-only (missing required
PG connection string -> clean `anyhow::Err` / non-zero exit, never a crash loop on a live
listener). The recurring real failure-mode divergences are:
(a) **auth-failure 400-vs-401** (communities, comms),
(b) **503-vs-200 when a writer/federation DB is unconfigured** (places, worlds),
(c) **500-instead-of-degrade** when a per-request DB dependency blips (media, market),
(d) **silent-ignore-vs-400** on invalid enum query params (market `/v1/bids`),
(e) **plain-text axum extractor bodies** instead of the JSON error envelope (media, comms,
archipelago malformed-POST).

---

## 4. User-capability matrix (current committed tree)

Corrects the earlier stale matrix. Scope = what an explorer user can actually do through
the catalyrst service plane. "Works" = client-called paths function end-to-end. "Partial"
= reads work, writes/some flows deferred or degraded. "Missing" = the user-facing action
cannot complete.

### Works (18)

| Capability | Service | Notes |
|---|---|---|
| Asset-bundle manifest + binary load | ab-cdn | byte-identical to S3+CloudFront; fs-error->404 |
| Scene AB registry resolution | ab-registry | reads serve; admin stub harmless |
| Badge display (achieved / not-achieved) | badges | arrays always emitted, no NRE |
| Marketplace browse (orders, sales, items, nfts, catalog, collections, prices, trendings, volume, rankings, trades) | market | all `{data,total}` shapes match; reads fully functional |
| Place discovery / lists / world list reads | places, lists | `GET` paths match (worlds `disabled` query-filter parity gap, no crash) |
| Event discovery (list, search, by-id) | events | the 4 client-called families are real handlers on `places_events` |
| Community member / bans / requests reads | communities | shapes match; auth-status + no-existence-check divergences (see §3) |
| Ban-status read (`/users/:address/bans`) | comms | `{data:{isBanned,ban?}}` matches DTO |
| World comms / scene comms token mint | worlds | `{fixedAdapter}` exact; missing 503 capacity gate only |
| Scene admin set read (`GET /scene-admin`) | comms | degrades to empty admin set on error, no crash |
| Translation (single + batch) | media | shapes match; `detectedLanguage` always present |
| Notifications read + subscription write | notifications | reads match; subscription input more-permissive |
| Friends / mutual / blocked / blocking-status / requests (social RPC) | social-rpc | shapes match; DB-error path throws (caught) |
| Eth RPC relay (wallet auth) | rpc | raw one-object WS frames match `DappWeb3Authenticator` |
| Credits progress / users / captcha | credits | captcha PNG raw bytes; wrong-captcha 200-vs-400 (UX only) |
| Explorer realm `about` (main) | explorer-api | synthetic About; client `Clear()`s before overwrite so absent fields safe |
| Map parcel chunks | map | the explorer-facing `ParcelChunkController` path is fine |
| Builder preview / newsletter | builder | fire-and-forget; no-op consumer |

### Partial (9)

| Capability | Service | Why partial |
|---|---|---|
| Realm `/about` health | content | 503-vs-200 on comms-down **breaks realm change** (must-fix #1) |
| Archipelago liveness | archipelago | `/status` 404 **blocks bootstrap** (must-fix #2) |
| Favorite / like a place | places | shape matches but **503-vs-200 when writer unset**; throws on client |
| Report a place | places | JSON shape OK but `signed_url` is a dead placeholder -> follow-up PUT throws |
| Community member listing | communities | works, but federated `role='admin'` row will throw the whole fetch (latent) |
| Translation under DB blip | media | 500-instead-of-degrade fails a translatable request |
| `/v1/bids` listing | market | flat shape (no `{ok,data}` wrapper) + invalid-param silently-ignored |
| Map NFT/district metadata | map | dApp-viewer JSON endpoints swallow DB errors to 404 (not explorer-facing) |
| Economy meta-tx | economy | validation runs, broadcast does not (documented behavioural gap) |

### Missing / deferred-to-federation (writes; none client-called) (5)

| Capability | Service | State |
|---|---|---|
| Curator place/world rating, ranking, highlight, featured | places | **501 curator_stub** (`federation.rs:133-207`); not in net-catalog |
| Event creation / edit / schedules / posters / profile settings | events | structured **501** federation-pending; event creation is a browser deep-link |
| Join community (`POST /v1/members/{address}/communities`) | communities | **501** after admin-bearer gate; no C# caller |
| World/place favorite-write via worlds alias | worlds/places | same writer-missing 503; not client-called |
| AB registry write (`POST /registry`) | ab-registry | stub echo; admin-only, not client-called |

---

## 5. Top 10 prioritized fixes

1. **`content GET /about`: make `comms` optional and stop gating `healthy` on it.**
   Emit `comms` only when configured; `healthy = content && lambdas` (+archipelago when
   set). Removes the 503-vs-200 realm-break. (`about.rs:307-322,366`.) — breaks-client.
2. **Mount `archipelago GET /status` on the explore bundle (5143) + add the nginx
   location.** Add `/status` to `api_router()` (`lib.rs:89-91`) and a proxy location in
   `nginx-catalyrst-bundles.conf`. Unblocks Unity bootstrap. — major.
3. **`communities` member-role: handle `admin`.** Add `Admin` to the C#
   `CommunityMemberRole` enum (or normalize/whitelist the role server-side in
   `ports/members.rs:14`) so a federated `role='admin'` row cannot throw the whole
   members fetch. — major (latent, federation-reachable).
4. **`communities` auth-failure status: return 401, not 400**, on missing/invalid auth
   chain for `GET /bans` and `GET /requests` (`bans.rs:20-21`, `requests.rs:29-30`) to
   match `signedFetchMiddleware`. — minor (correctness).
5. **`comms` auth-failure status: return 401, not 400**, across all scene-admin /
   scene-bans routes (`scene_admin.rs:29/64/75`, `scene_bans.rs:51/84/108/118`). — minor.
6. **`places` writer-missing path: stop returning 503** on favorites / likes / report
   when the writer DB is unconfigured (`ports/places.rs:304-308,348-352,425-429`).
   Either require the writer in the intended deployment or degrade without a status the
   client throws on. — minor (only bites when writer unset).
7. **`places POST /api/report`: issue a real presigned URL** (or document the flow as
   disabled) — the current `?federation=pending` placeholder makes the client's mandatory
   follow-up PUT throw (`report.rs:28`, `PlacesAPIClient.cs:475`). — minor.
8. **`market /v1/bids`: emit the `{ok:true,data:{...}}` wrapper** to match
   `bids-handler.ts:28-40` (sole wrapper break in the crate), and use the throwing
   `get_parameter` helper (`http/pagination.rs:42-58`) so invalid `sortBy/network/status`
   -> 400 instead of a silently-ignored 200. — minor.
9. **`media` cache DB: make it best-effort.** A transient cache-DB blip should not 500 a
   translatable request (`handlers/translate.rs:139,182` call cache with `?`). Fall back to
   serving the backend translation. — medium degradation gap.
10. **Standardize error bodies + axum extractor rejections** to the JSON envelope: wire a
    `JsonRejection -> ApiError` mapper (media, comms, archipelago malformed-POST currently
    leak plain-text 400/422), and normalize the comms/communities `{ok:false,message}` vs
    upstream `{message}`/`{error}` body shape. Also fix the `events` sitemap 501 to use the
    envelope. — minor but pervasive consistency.
