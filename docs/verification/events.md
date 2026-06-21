# catalyrst-events (service "events") — adversarial re-verification

Branch: `feat/service-plane-crates`
Crate: `crates/catalyrst-events`
Upstream: `decentraland/events`
Unity consumer: `decentraland/unity-explorer/Explorer/Assets/DCL/EventsApi/HttpEventsApiService.cs`
Net-catalog: `the Unity net-catalog`

## Verdict

The submitted findings are **substantially correct**. Every shape verdict and every "client_reaction: ok" holds up on the committed tree. The Unity client only calls four endpoint families (confirmed against the net-catalog), all of which are real handlers backed by the `places_events` DB; every divergent endpoint is on a path the client never touches. **No client-crash risk exists.** One failure-mode claim is correct in outcome but wrong in mechanism (see Failure-mode gaps). The "sole error-model inconsistency" (sitemap bare text/plain 501) is confirmed.

## Client surface (net-catalog `endpoints` table, file `HttpEventsApiService.cs`)

Only these are real HTTP API calls; everything else under EventsApi is `OPEN_URL` browser deep-links (whats-on/new-event, google calendar, twitter) — not service calls.

| Method | URL | C# method |
|---|---|---|
| GET | `/api/events[?list=live]` | `GetEventsAsync` → `FetchEventListAsync` |
| GET | `/api/events?positions[]=...[&list=live]` | `GetEventsByParcelAsync` → `FetchEventListAsync` |
| GET | `/api/events?from=&to=&with_connected_users=` | `GetEventsByDateRangeAsync` (no failure handler, no ok-guard) |
| GET | `/api/events?limit=&offset=&list=highlight&with_connected_users=` | `GetHighlightedEventsAsync` (no failure handler, no ok-guard) |
| GET | `/api/events?community_id=&limit=&offset=` | `GetCommunityEventsAsync` (envelope-with-total) |
| POST | `/api/events/search?limit=&offset=` JSON `{communityId,placeIds[]}` | `GetCommunityEventsByPlaceIdsAsync` (envelope-with-total) |
| POST | `/api/events/{eventId}/attendees` | `MarkAsInterestedAsync` (reads only `AttendResponse{ok}`) |
| DELETE | `/api/events/{eventId}/attendees` | `MarkAsNotInterestedAsync` (reads only `AttendResponse{ok}`) |

`GET /api/events/categories`, `/api/schedules*`, `/api/poster*`, `/api/profiles/*`, `/events/sitemap*`, `POST/PATCH /api/events`, `PATCH /api/events/{id}` — **not in the catalog. Confirmed never client-called.**

## Per-endpoint table

| endpoint | shape | client-reaction | severity | failure-modes-ok | notes |
|---|---|---|---|---|---|
| GET /api/events | match | ok | minor | mostly (see gaps) | Bare `{ok,data:[EventRecord]}`; `{ok,data:{events,total}}` when `places_ids`/`community_id` present (events.rs:143-152). EventRecord snake_case ⊇ EventDTO; all client-read fields present & typed. `connected_addresses` omitted unless `with_connected_users` (`skip_serializing_if`). |
| POST /api/events/search | match | ok | minor | yes | Same dual envelope; client always hits the `{events,total}` branch (sends placeIds/communityId). Maps to upstream `POST /events/search`. |
| GET /api/events/attending | match | ok | minor | yes | 401 if no signer (events.rs:204-205); else `{ok,data:[EventRecord]}`. Shape-safe. |
| GET /api/events/{id} | match | ok | minor | yes | 404 `Not found event "id"` if missing or `!approved`. |
| POST /api/events | divergent (501) | ok | minor | yes | 501 JSON envelope. Upstream `withAuth+withAuthProfile` → 201 (Event/routes/index.ts:67). Web deep-link only, not client-called. |
| PATCH /api/events/{id} | divergent (501) | ok | minor | yes | 501 (EventModerate pending). Upstream moderates. Not client-called. |
| GET /api/events/categories | divergent | ok | minor | yes (fail-soft) | In-use categories only, English-only i18n, synthesized `now()` timestamps (categories.rs:55-71). Upstream full static table. DB error on subquery → `unwrap_or_default` → 200 `[]`. Not client-called. |
| POST /api/events/{id}/attendees | divergent | ok | minor | yes | `{ok,data:[EventAttendeeRecord]}`; client reads ONLY `AttendResponse{ok}` (HttpEventsApiService.cs:224-227) — data ignored, divergence invisible. Upstream also returns the list (`getEventAttendeeList`, EventAttendee/routes.ts:73). 401/404/500 aligned. |
| DELETE /api/events/{id}/attendees | divergent | ok | minor | yes | Same: list returned, client reads only `ok`. Upstream returns list too (routes.ts:84). |
| GET /api/schedules | divergent (stub) | ok | minor | yes | Real handler but port returns `Ok(Vec::new())` → 200 `{ok,data:[]}` (ports/schedules.rs:15-17). Upstream serves rows. Not client-called. |
| POST /api/schedules | divergent (501) | ok | minor | yes | 501. Upstream `withAuth` → 201. Not client-called. |
| GET /api/schedules/{id} | divergent (stub) | ok | minor | yes | Port returns `Ok(None)` → always 404 `Schedule "id" not found`. Not client-called. |
| PATCH /api/schedules/{id} | divergent (501) | ok | minor | yes | 501. Not client-called. |
| POST /api/poster | divergent (501) | ok | minor | yes | 501 (S3 not provisioned). Not client-called. |
| POST /api/poster-vertical | divergent (501) | ok | minor | yes | 501. Not client-called. |
| GET /api/profiles/settings | divergent (501) | ok | minor | yes | 501 (admin/no moderators table). Not client-called. |
| GET /api/profiles/me/settings | divergent (501) | ok | minor | yes | 501. Not client-called. |
| PATCH /api/profiles/me/settings | divergent (501) | ok | minor | yes | 501. Not client-called. |
| GET /api/profiles/{id}/settings | divergent (501) | ok | minor | yes | 501. Not client-called. |
| PATCH /api/profiles/{id}/settings | divergent (501) | ok | minor | yes | 501. Not client-called. |
| GET/POST/DELETE /api/profiles/subscriptions | divergent (501) | ok | none | yes | 501; upstream @deprecated web-push. Not client-called. |
| GET /events/sitemap.xml | divergent (501, **bare text**) | ok | minor | yes | **501 `text/plain`, NOT the JSON envelope** (sitemap.rs:4-9) — sole error-model inconsistency. Crawler-only, not client-called. |
| GET /events/sitemap.static.xml | divergent (501 bare text) | ok | minor | yes | Same stub. Not client-called. |
| GET /events/sitemap.events.xml | divergent (501 bare text) | ok | minor | yes | Same stub. Not client-called. |
| GET /events/sitemap.schedules.xml | divergent (501 bare text) | ok | minor | yes | Same stub. Not client-called. |

## Confirmed issues

1. **Sitemap stubs return bare `text/plain` 501, not the `{ok:false,error}` JSON envelope** (severity: low). `handlers/sitemap.rs:4-9` returns a 2-tuple `(StatusCode, &str)`. Single deviation from the otherwise-coherent `ApiError → {ok:false,error}` model (`http/response.rs:55-69`). Crawler-only, never client-called, never JSON-parsed in-client. Cosmetic; flag-and-leave.

2. **Categories endpoint synthesizes `created_at`/`updated_at` as `Utc::now()`, ships English-only i18n, filters to in-use categories** (severity: low). `ports/categories.rs:55-71`. Real divergence from upstream's static full table with real timestamps + full-locale i18n. Never client-called. No consumer reads these fields.

3. **Schedules are hard stubs** (`GET /api/schedules` → `[]`; `GET /api/schedules/{id}` → 404 always). `ports/schedules.rs` holds the pool but issues no query. Real divergence; never client-called.

4. **All write/admin/federation endpoints are 501** (POST/PATCH events, schedules, posters, all profile settings, subscriptions), each a structured 501 JSON envelope with a federation-pending explanation. Upstream serves these (201/200). None client-called (browser deep-links handle event creation). Expected pre-federation.

All four are **accepted divergences on never-client-called paths**, not defects against the Unity client.

## Client-crash risks

**None.** Verified end-to-end:

- `EventDTO`/`EventWithPlaceIdDTO` are `[Serializable]` value types; `EventDTOListResponse`/`EventWithPlaceIdDTOListResponse` are structs with nullable reference members. `data` read as `result.data ?? Array.Empty<EventDTO>()` on every list path — null-safe.
- `EventDataParser.ParseDeserializedDates` uses `DateTime.TryParse` (null-tolerant) and guards `Recurrent_dates == null`. No non-null assertion in the converter.
- Our `EventRecord` is a superset of `EventDTO`: every client-read field (`id,name,image,description,next_start_at,next_finish_at,finish_at,scene_name,coordinates,server,total_attendees,live,user_name,highlighted,trending,attending,categories,recurrent,duration,start_at,recurrent_dates,world,x,y,place_id,connected_addresses,community_id,image_vertical`) present and correctly typed (`estate_name` falls back to `scene_name`; `coordinates`/`position` both emitted as `[x,y]`). Extra fields ignored by JsonUtility.
- Attend paths read only `AttendResponse{ok}`; a non-2xx never reaches that read.

## Failure-mode gaps

1. **Original finding #1 failure-mode #4 (DB-down on date/highlight client path) is correct in outcome but wrong in mechanism — and there is NO observable divergence.** The finding claims `GetEventsByDateRangeAsync`/`GetHighlightedEventsAsync` "don't guard `ok` nor supply a failure handler so `CreateFromJson` throws." Verified mechanism: on any non-2xx, `WebRequestController.SendAsync` throws `UnityWebRequestException` from `request.SendRequest` (WebRequestController.cs:77) and re-throws after retries (line 113); `op.ExecuteAsync` — where `CreateFromJson`'s parse + custom-failure logic lives (GenericDownloadHandlerUtils.cs:212-260) — is **never invoked on an HTTP error**. Consequences:
   - It is `SendAsync`, not `CreateFromJson`, that throws on our 500.
   - The `createCustomExceptionOnFailure` handler in `FetchEventListAsync` (live/parcel path) only fires on a JSON-parse failure of a **2xx** body; it does NOT catch HTTP 500s. Its presence vs. absence is irrelevant to DB-down behavior.
   - Both code paths therefore behave **identically** on our 500: a thrown `UnityWebRequestException`, surfaced as a handled error (logged + retried + propagated), not a crash. The `response.ok` guard the date/highlight paths skip is dead weight: our service only ever sets `ok:false` together with a non-2xx status, so a 2xx body always carries `ok:true`. There is no scenario where our service returns 200 + `{ok:false}`.
   - Net: the divergence the finding flags is not observable. The underlying claim "client doesn't crash, error is handled" stands; downgrade the significance of the date/highlight-vs-list distinction.

2. **`total` never 500s on the envelope-with-total path.** Count subquery uses `unwrap_or(0)` (ports/events.rs:232) — fail-soft. If the main `fetch_all` succeeds but the count fails, client gets a valid `{events,total:0}`. Harmless.

3. **`approved_visibility()` is hard-coded `false`** (ports/events.rs:386-388), always appending `AND approved IS TRUE` regardless of `approved`/`rejected` query params. Upstream lets authorized callers see unapproved events. Client never sends those params on read paths → no observable divergence; noted for completeness.

## Crate-level (confirmed)

- **Startup: panic-free but not degradeable.** `config.rs:17` makes `PLACES_EVENTS_PG_CONNECTION_STRING` mandatory (`main()` exits non-zero via anyhow if missing). `lib.rs:33-46` eagerly `connect_with().await.context()`s the single Postgres pool before binding — unreachable DB at boot exits the binary before the listener comes up. No optional DBs, no lazy mode. HTTP defaults `127.0.0.1:5135`.
- **Runtime DB failures degrade to 500**, body `{"ok":false,"error":"database error"}` (real error logged via `tracing::error!`, not leaked — response.rs:60-63). All query handlers use `?`; no `.unwrap()` on query results. `total` count and categories subquery are fail-soft.
- **Error model coherent**: `ApiError → IntoResponse` always emits `{"ok":false,"error":msg}` with mapped status (400/401/404/500/501), aligned with upstream well-known-components convention. **Sole exception: the sitemap stub's bare text/plain 501.**
- No oversize-body guard beyond axum defaults.
