# Adversarial verification — catalyrst-places (service "places")

Scope: re-check of the prior re-check findings for the places parity surface against the
**committed** tree on `main` (crate `crates/catalyrst-places`), the upstream TS
(`github.com-decentraland/places`), and the Unity consumer
(`unity-explorer/.../PlacesAPIService/PlacesAPIClient.cs`) plus the net-catalog
(`the Unity net-catalog`).

Verdict: **all flagged findings hold up.** None rejected as cosmetic/stale/already-fixed. The two
client-impacting items are both *functional degradation when the writer DB is unconfigured* (a 503
status upstream never emits) and the *non-functional report signed_url*. Every curator-route
divergence is real but on endpoints the Unity client never calls (confirmed absent from the
net-catalog), so severity "none" is correct.

## Code anchors confirmed
- Routes: `src/lib.rs:100-187` — API nested under `/api`, social under `/places`, `/health` at root.
- Error model: `src/http/errors.rs` — `{ok:false,message}`; 501/503 merge `federation_adr`; sqlx → 500 `"database error"` (no leak). Matches gatsby ErrorResponse `{ok:false,message}`.
- Writer-missing 503: `src/ports/places.rs:304-308` (favorites), `:348-352` (likes), `:425-429` (report) — all `service_unavailable(...)` when `writer.is_none()`.
- Curator stubs: `src/handlers/federation.rs:133-207` — `curator_stub()` → `not_implemented` (501) for rating/ranking/highlight/featured on both places and worlds.
- Report handler: `src/handlers/report.rs:28-38` — builds `https://places-report-uploads.decentraland.org/{filename}?federation=pending`, returns `{ok:true,data:{signed_url}}` (omits filename).
- Auth: `src/auth.rs` — reads only `x-identity-auth-chain-0` payload, no signature verification; `auth_address_required` → 401 `"Invalid authentication"`.
- Startup: `src/config.rs:18` reader URL `required()`; `src/lib.rs:33-38` reader pool `connect_with(...).await?` (graceful Err, no panic); writer/squid optional with warn+continue.

## Per-endpoint table

| Endpoint | Shape | Client reaction | Severity | Failure-modes OK? | Notes |
|---|---|---|---|---|---|
| PATCH /api/places/{id}/favorites | match | request-throws on non-2xx (no deserialize) | minor | all OK **except** writer-missing 503 | `SetPlaceFavoriteAsync` (PlacesAPIClient.cs:379) only `.WithCustomExceptionAsync`, no `ignoreErrorCodes` → any non-2xx throws. Upstream `UserFavorite/routes.ts:61` returns `{favorites,user_favorite}` + idempotent short-circuit — exact match. |
| PATCH /api/places/{id}/likes | match | request-throws on non-2xx | minor | all OK **except** writer-missing 503 | `RatePlaceAsync` (cs:398) sends `{like:true|false|null}`, no deserialize. Upstream `UserLikes/routes.ts:66` returns `{likes,dislikes,user_like,user_dislike}` + short-circuit — match. |
| PUT /api/places/{id}/rating | divergent (501 vs 200) | n/a — **not client-called** | none | OK | Curator route exists upstream `Place/routes/index.ts:38`. Absent from net-catalog. |
| PUT /api/places/{id}/ranking | divergent (501 vs 200) | not client-called | none | OK | Upstream `:39`. Not in catalog. |
| PUT /api/places/{id}/highlight | divergent (501 vs 200) | not client-called | none | OK | Upstream `:40`. Not in catalog. |
| PUT /api/places/{id}/featured | divergent (501 vs 200) | not client-called | none | OK | Upstream `:42`. Not in catalog. |
| DELETE /api/places/{id}/featured | divergent (501 vs 200/204) | not client-called | none | OK | Upstream `:43`. Not in catalog. |
| PATCH /api/worlds/{id}/favorites | match | not client-called (client only GETs /api/worlds) | minor | writer-missing 503 diverges | Delegates to `patch_place_favorites`. Upstream `World/routes/index.ts:38`. Not in catalog. |
| PATCH /api/worlds/{id}/likes | match | not client-called | minor | writer-missing 503 diverges | Delegates to `patch_place_likes`. Upstream `:39`. Not in catalog. |
| PUT /api/worlds/{id}/highlight | divergent (501 vs 200) | not client-called | none | OK | Upstream `:40`. Not in catalog. |
| PUT /api/worlds/{id}/ranking | divergent (501 vs 200) | not client-called | none | OK | Upstream `:41`. Not in catalog. |
| PUT /api/worlds/{id}/rating | divergent (501 vs 200) | not client-called | none | OK | Upstream `:42`. Not in catalog. |
| PUT /api/worlds/{id}/featured | divergent (501 vs 200) | not client-called | none | OK | Upstream `:43`. Not in catalog. |
| DELETE /api/worlds/{id}/featured | divergent (501 vs 200/204) | not client-called | none | OK | Upstream `:44`. Not in catalog. |
| POST /api/report | match (both omit filename) | parse-OK, then **PUT to signed_url fails** | minor | writer-missing 503 + non-functional signed_url diverge | `ReportPlaceAsync` (cs:452) reads `response.ok`+`data.signed_url`, then PUTs payload there (cs:475). Our URL is a `federation=pending` placeholder → PUT throws. Upstream `Report/routes.ts:67` returns a real S3 putObject URL, also omitting filename. |
| GET /health | divergent (operational) | not client-called | none | OK | `{ok,version,components:{places_db}}`, 200/503 from `SELECT 1`. Not a parity surface. |
| GET /places/place/ (social meta) | divergent (HTML SSR) | not client-called | none | OK | OG/twitter meta page; missing place → generic 200 page, never 404; DB errors swallowed via `.ok().flatten()` (social.rs:58-63). Not a JSON API. |

## Confirmed issues (real on committed tree)

1. **Writer-missing 503 on PATCH favorites / PATCH likes / POST report** — `set_favorite`/`set_like`/`record_report` return `ServiceUnavailable` (503 + `federation_adr`) when `writer` is `None` (`ports/places.rs:304,348,425`). Upstream returns **200** and persists. This status upstream never emits on these routes. On the Unity client it becomes a thrown `PlacesAPIException` (favorites/likes) because `PatchAsync(...)` is called with no `ignoreErrorCodes` (cs:388,415). Severity minor: only triggers when `PLACES_PG_COMPONENT_WRITER_PSQL_CONNECTION_STRING` is unset/invalid/unreachable; with the writer configured (the intended deployment) these are real 200s and the shapes match exactly.

2. **POST /api/report returns a non-functional signed_url** — `report.rs:28` always yields `https://places-report-uploads.decentraland.org/{filename}?federation=pending`. The POST succeeds with `{ok:true,data:{signed_url}}` (shape matches upstream — both omit `filename`), but the client's mandatory follow-up `PutAsync(response.data.signed_url, ...)` (`PlacesAPIClient.cs:475`) targets that placeholder host, which is not a writable presigned S3 URL → PUT 4xx/5xxs → `ReportPlaceAsync` throws. End-to-end report flow is broken even though the JSON shape is correct. Functional degradation, not a shape break. Severity minor (rarely-hit UI action).

## Client-crash risks

None of the response *shapes* null-crash the client:
- favorites/likes responses are **not deserialized** (`SetPlaceFavoriteAsync`/`RatePlaceAsync` only `.WithCustomExceptionAsync`), so no converter / non-null assertion to trip. Non-2xx → thrown `PlacesAPIException`, handled by callers as an error, not an engine crash.
- report: `ReportPlaceResponseData` (`ReportPlaceAPIResponse.cs:13-17`) is `string filename; string signed_url;` — plain nullable strings, no `[JsonRequired]`/non-null assertion. `filename` stays null on both ours and upstream (JsonUtility tolerates the missing field). Client reads only `response.ok` and `response.data.signed_url`.

The only request-level failures that surface are **thrown exceptions**: a 503 on favorites/likes when the writer is down, and the failed report PUT.

## Failure-mode gaps (where our error path diverges from upstream)

- **503 (writer unconfigured) on PATCH favorites, PATCH likes, POST report** — upstream returns 200 and persists; we throw on the client. Confirmed `ok:false`.
- **POST /report signed_url** — upstream's URL is a working presigned S3 PUT; ours is a `federation=pending` dead end → client PUT fails. Confirmed `ok:false`.
- **All other failure modes verified `ok:true`**: 401 on missing/invalid auth (`auth_address_required` → 401), 400 on malformed `{favorites}`/`{like}` bodies (`federation.rs:32,70`), 404 on missing entity (`federation.rs:40,77`), 500 on writer-down-at-write-time (sqlx → `Database` → 500). These match upstream status classes.
- **GET /places/place/ social meta** swallows DB errors (`.ok().flatten()` in `social.rs:58-63`) and renders a generic 200 page rather than 404/5xx — divergent but SSR-only and not client-called, so no client impact.

## Startup / crate-level (re-confirmed)
Panic-free but not unconditionally start-clean: `Config::from_env()` hard-requires `PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING` (`config.rs:18`), and `build_state()` opens the reader pool with `connect_with(...).await?` (`lib.rs:33-38`) — an unreachable reader DB at boot fails startup with a graceful `Err` (non-zero exit, no panic). Writer + squid are optional: absent/invalid/unreachable → `warn` + continue. With no writer, reads quietly no-op user interactions (`user_favorite/like/dislike` stay false; `apply_user_interactions` early-returns when `writer.is_none()`) and writes 503; with no squid, the `owner` filter returns `[]`. `ensure_local_schema()` failures are `warn`, not fatal. HTTP defaults 127.0.0.1:5134. No LiveKit dependency. Net: only the primary reader DB is a hard startup dependency; everything else degrades cleanly.

## Residual observation (outside this lane's flagged set)
GET /api/worlds: the Unity `GetWorldsAsync` query sends a `disabled` param (PlacesAPIClient.cs:168) that our `get_world_list` (`worlds.rs:62-73`) does not parse; upstream supports it. Not part of the favorites/likes/curator/report findings under verification, but a real list-filter parity gap on a client-called read endpoint (no crash — extra query params are ignored, returns active worlds).
