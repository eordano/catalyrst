# Parity report: catalyrst-places vs upstream `decentraland/places`

Service: **places** (port 5134). Rust crate: `crates/catalyrst-places`.
Upstream TS: `decentraland/places`.
Unity client surface verified against `Explorer/Assets/DCL/PlacesAPIService/` (`PlacesAPIClient.cs`, `PlacesAPIResponse.cs`) and `the Unity net-catalog`.

This is an **adversarial re-verification** of a prior finding set. Each flagged divergence was confirmed by reading both the Rust struct/handler and the upstream TS shape, and cross-checking what the explorer actually deserializes. Verdicts that survived are listed; corrections and rejections are called out at the bottom.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /ping | match | same | none | Plain text echo both sides; nothing depends on it. |
| GET /api/status | match | same | none | `{ok,data:{image,timestamp,version}}` matches. Value-only diffs (image string, ts format). Upstream mounts at `/status` root; ours at `/api/status` (prefix only). |
| GET /api/categories | divergent | better* | major | STUB `data:[]`. Upstream: 2 SQL (findActiveCategories + findActiveCategoriesWithPlaces) + en.json merge -> `[{name,active,count,i18n:{en}}]`, honors `?target`. Empty array = no categories. "better" is degenerate (does nothing). |
| GET /api/places/{place_id} | divergent | better (qualified) | major | Envelope matches. Narrow SELECT omits real columns `world_id, sdk, ranking, highlighted_image, disabled_reason` that upstream returns via `SELECT p.*`; `realms_detail`/`is_private` not modeled. `user_*` hardcoded false, `user_count` null, `user_visits` 0. Unity `PlaceInfo` DOES read `world_id`(via world_name), `sdk`-less badge, `realms_detail`, `highlighted_image`, `connected_addresses`, `featured` -> degraded UI, no crash (JsonUtility tolerant). |
| GET /api/places | divergent | better (qualified) | major | Same per-element gaps. Missing filters: `only_favorites, with_realms_detail, owner(operated-lands), names, sdk, MOST_ACTIVE/USER_VISITS order_by`. `search<3` short-circuits; ILIKE substring vs upstream ts_rank_cd -> different result set/order. Envelope `{ok,data,total}` matches. |
| POST /api/places | divergent | better | minor | Array+max-100 validation matches. Same element gaps. Ignores limit/offset/order_by/search (fine for Unity `GetPlacesByIdsAsync` which just wants ids back). |
| POST /api/places/status | match | same | none | `{id,disabled,world,world_name,base_position}` matches upstream Pick. 2 concurrent queries both sides. |
| GET /api/places/{place_id}/categories | match | better* | minor | STUB `{categories:[]}`. Inner shape matches; permanently empty (upstream findCategoriesByPlaceId). |
| PATCH /api/places/{id}/favorites | divergent | n/a | breaks-client | 501 vs upstream 200. Unity `SetPlaceFavoriteAsync` does NOT parse the body but the 501 makes the HTTP call throw `PlacesAPIException` -> favorite toggle fails. By-design federation stub. |
| PATCH /api/places/{id}/likes | divergent | n/a | breaks-client | 501 vs upstream 200. Unity `RatePlaceAsync` ignores body, but 501 -> throws -> like/dislike fails. Federation stub. |
| PUT /api/places/{id}/rating | divergent | n/a | minor | 501. Admin/curator-only; not a Unity client path. |
| PUT /api/places/{id}/ranking | divergent | n/a | minor | 501. Data-team bearer-token; not a client path. |
| PUT /api/places/{id}/highlight | divergent | n/a | minor | 501. Admin-only; not a client path. |
| PUT /api/places/{id}/featured | divergent | n/a | minor | 501. Admin bearer; not a client path. |
| DELETE /api/places/{id}/featured | divergent | n/a | minor | 501. Admin bearer; not a client path. |
| GET /api/worlds/{world_id} | divergent | n/a | minor (was major) | STUB 404 vs upstream 200 AggregateWorldAttributes. Real API divergence, BUT the Unity client resolves world detail via `/api/places?positions=&names=` (`GetWorldAsync`), not `/api/worlds/{id}`. Low Unity blast radius -> downgraded. |
| GET /api/worlds | divergent | better* | major | STUB `{ok,data:[],total:0}`. Unity `GetWorldsAsync` reads this envelope; empty array (not null) parses cleanly -> worlds list always empty. Upstream findWorldsWithAggregates+countWorlds + worldsLiveData cache. |
| GET /api/world_names | divergent | better* | major | STUB empty. Upstream `string[]` of world names. Empty -> world-name lookups blank. |
| PATCH /api/worlds/{id}/favorites | divergent | n/a | breaks-client | 501. World-favorite toggle throws until federation write path. |
| PATCH /api/worlds/{id}/likes | divergent | n/a | breaks-client | 501. World like/dislike throws until federation. |
| PUT /api/worlds/{id}/highlight | divergent | n/a | minor | 501. Admin-only. |
| PUT /api/worlds/{id}/ranking | divergent | n/a | minor | 501. Bearer-token data path. |
| PUT /api/worlds/{id}/rating | divergent | n/a | minor | 501. Admin-only. |
| PUT /api/worlds/{id}/featured | divergent | n/a | minor | 501. Bearer admin. |
| DELETE /api/worlds/{id}/featured | divergent | n/a | minor | 501. Bearer admin. |
| POST /api/report | divergent | n/a | breaks-client | 501 vs upstream 200 `{data:{signed_url}}`. Unity `ReportPlaceAsync` step 1 needs the signed URL (checks `response.data == null || !response.ok`) -> 501 aborts the report-upload flow. Federation stub. |
| GET /api/map | divergent | better* | major | STUB `data:{}` (empty object). Upstream `placesWithCoordinatesAggregates` returns a coordinate-KEYED object `Record<base_position, place>` -> our empty `{}` is the empty case of that shape (CONFIRMED object, not array). Unity `GetOptimizedPlacesFromTheMapAsync` -> empty -> no minimap overlay. (See caveat: Unity parses this as a bare Newtonsoft `List<>`, which mismatches even upstream's enveloped object; pre-existing, not a catalyrst regression.) |
| GET /api/map/places | divergent | better* | major | STUB `{data:[],total:0}`. Upstream places UNION worlds list + getSceneStats + hotScenes/worldsLiveData. Empty. |
| GET /api/destinations | divergent | better* | major | STUB empty. Upstream places+worlds + up to 3 external HTTP (operated-lands, connected-users, live-events). Unity `GetDestinationsAsync` -> empty Discover rail. |
| POST /api/destinations | divergent | better* | major | STUB ignores body, returns empty; does NOT enforce upstream's array max-100 validation. Unity `GetDestinationsByIdsAsync` -> empty. |
| GET /places/place/ | divergent | better* | minor | Empty SOCIAL_HTML_TEMPLATE: blank title/description, no og:* meta, no canonical Link header. Upstream injects helmet meta from PlaceModel. SEO/social-card crawlers get blank. |
| GET /places/world/ | divergent | better* | minor | Same empty template for worlds. Blank og meta, no canonical Link header. |

`*` = "better" only because the stub does no work (degenerate / correctness-degrading), not a legitimate structural win.

## Confirmed shape issues

1. **`/api/categories` is a permanent empty stub.** Upstream `Category/routes.ts` runs 2 SQL + merges `intl/en.json` translations and honors `?target=places|worlds|all`, returning `[{name, active, count, i18n:{en}}]`. Rust returns `{ok:true,data:[]}`. (Prior finding under-listed the element: upstream also emits `active`.)

2. **`PlaceRow` omits real upstream columns.** Upstream `findWithAggregates`/`findByIdWithAggregates` select `p.*`, so `world_id, sdk, ranking, highlighted_image, disabled_reason` are all returned (`PlaceAttributes` in `Place/types.ts`). The Rust `find_by_id`/`find_by_ids`/`find_list` use an explicit narrow column list (`ports/places.rs`) that never selects these. `realms_detail` and `is_private` (`AggregatePlaceAttributes`) are likewise absent. **The Unity `PlaceInfo` DTO (`PlacesAPIResponse.cs`) explicitly declares and reads `world_name, is_private, highlighted_image, featured, featured_image, realms_detail, live, connected_addresses, tags`** — so these omissions degrade the navmap/place-info UI (no SDK badge, no realms list, no live/connected users, no featured imagery). Parsing does not crash because the client uses `WRJsonParser.Unity` (JsonUtility), which tolerates missing/extra fields. Confirmed real.

3. **`user_*` interaction fields hardcoded.** `row_to_place` sets `user_favorite/user_like/user_dislike=false`, `user_count=None`, `user_visits=0`. Upstream LEFT-JOINs `user_favorites`/`user_likes` for the optional auth user (`model.ts` lines ~335-360) and fills `user_count` from hotScenes + `user_visits` from getSceneStats. Confirmed real; affects "favorited/liked" UI state.

4. **`/api/worlds`, `/api/world_names`, `/api/map`, `/api/map/places`, `/api/destinations` (+POST) are empty stubs.** All upstream routes exist and return populated data (verified route files under `World/routes`, `Map/routes`). Client-visible effect: empty worlds list, empty Discover/destinations rail, no minimap category overlay.

5. **`/api/worlds/{world_id}` returns 404; upstream returns 200 `AggregateWorldAttributes`.** Real API divergence. Downgraded from major to minor for Unity specifically because the client's world-detail path is `/api/places?positions=&names=` (`GetWorldAsync`), not this route.

6. **Federation write stubs return 501.** favorites/likes (place + world) and report all 501. For favorites/likes the Unity callers (`SetPlaceFavoriteAsync`, `RatePlaceAsync`) ignore the response body but a 501 makes the request throw -> the action fails. For report, `ReportPlaceAsync` explicitly checks `response.data == null || !response.ok` -> 501 aborts. All breaks-client until the federation write path lands. Admin/curator PUT/DELETE 501s are not client paths (minor).

7. **Social templates serve blank metadata.** `/places/place/` and `/places/world/` return a static empty `SOCIAL_HTML_TEMPLATE` with no og:* tags and no canonical `Link` header; upstream injects helmet meta from PlaceModel/WorldModel. SEO/social-card only; no explorer impact.

## Confirmed efficiency wins (with structural reason)

These are legitimate structural wins (NOT language choice), confirmed by reading both implementations:

1. **GET /api/places/{place_id}** — Rust runs **1 query** with no user-interaction LEFT JOINs (`find_by_id`). Upstream `findByIdWithAggregates` adds 2 conditional LEFT JOINs (`user_favorites`, `user_likes`) when an auth user is present. Structural fewer joins. **Qualifier:** the upstream `getHotScenes()`/`getSceneStats()` enrichment is in-memory cached (`modules/hotScenes.ts` = setInterval-refreshed memory; `modules/sceneStats.ts` = `memo` ttl 1h), so it is NOT a per-request scan — the win is the absence of the interaction joins, not avoidance of heavy enrichment. Partly achieved by omitting data.

2. **GET /api/places** — Rust runs **2 queries concurrently** via `tokio::try_join!` (`find_list` + `count_list`), simple WHERE, no per-row interaction joins. Upstream does `Promise.all` of **3** (`findWithAggregates` w/ interaction subquery + `countPlaces` + `getSceneStats`) **plus a synchronous external HTTP call to `CatalystAPI.getAllOperatedLands` when `owner` is set** (confirmed in `getPlaceList.ts` lines 90-107). The extra HTTP and the interaction joins are the real structural differences. Partly achieved by dropping the `owner`/operated-lands feature.

3. **POST /api/places** — Rust: 2 concurrent queries (`find_by_ids WHERE id=ANY($1)` + `count_by_ids`), single ANY() lookup, no interaction joins. Upstream `getPlaceListById` -> `findWithAggregates` (full aggregate subquery w/ user joins) + `countByIds`. Fewer joins. Real.

4. **POST /api/places/status** — equivalent, both 2 narrow queries (not a "win", listed as same).

The remaining "better" verdicts (categories, place-categories, worlds, world_names, map, map/places, destinations, social) are **degenerate**: cheaper only because the stub returns nothing. These are correctness regressions, not structural wins, and should not be counted as legitimate efficiency improvements.

## Rejected / corrected during verification

- **REJECTED (new issue not added): `like_rate`/`like_score` type mismatch with Unity.** The Unity DTO field is `string like_rate` while both Rust and upstream emit a JSON **number** (`like_rate` is a numeric column, migration `1680794073381` shows numeric default 0.5; TS type `number | null`). This number->string-field quirk is **identical** for upstream and catalyrst, so it is not a catalyrst divergence. The prior finding's "shape matches (both number|null), value-divergent" verdict is correct and was kept. No new issue created.

- **CORRECTED: `/api/map` data type.** Prior finding said "object vs upstream's coordinate-aggregated structure" — verified that upstream `placesWithCoordinatesAggregates` (`Map/utils.ts`) returns a `Record<base_position, place>` **object**, so our empty `{}` is the empty case of the SAME object shape, not a type mismatch. The shape verdict (divergent-by-emptiness) stands; the "object vs ?" framing was imprecise.

- **CAVEAT added: `/api/map` Unity parse.** The Unity `GetOptimizedPlacesFromTheMapAsync` parses the response as a **bare Newtonsoft `List<OptimizedPlaceInMapResponse>`**, which does not match upstream's enveloped coordinate-keyed object either. This is a pre-existing client/upstream mismatch, not a catalyrst regression — noted but not charged against catalyrst.

- **DOWNGRADED: `/api/worlds/{world_id}` major -> minor.** Real API divergence (404 vs 200), but the Unity client does not call this route for world detail (it uses `/api/places?positions=&names=`). Lower client blast radius than the prior "any world detail lookup fails" claim.

- **REFINED: favorites/likes "expects 200 {ok:true,data:{...}}" body shape.** Verified the Unity callers (`SetPlaceFavoriteAsync`, `RatePlaceAsync`) are `UniTask` (void) and never deserialize the response body — they only attach exception wrappers. The breaks-client outcome holds (501 -> the request throws), but the break is the HTTP error status, not a body-shape mismatch. report (`ReportPlaceAsync`) genuinely reads `response.data.signed_url` and checks `!response.ok`, so its body dependency is real.

- **REFINED: enrichment cost.** Several efficiency rationales implied upstream pays per-request `getHotScenes()`/`getSceneStats()` cost. Verified both are in-process caches (interval memory / memo TTL), so the structural win is the absence of interaction LEFT JOINs and (for list) the operated-lands HTTP, not avoidance of enrichment scans.
