# Parity report: catalyrst-events vs upstream `decentraland/events`

Service: **events** (`catalyrst-events`, port 5135).
Rust crate: `crates/catalyrst-events`.
Upstream TS: `decentraland/events`.
Verified statically (upstream events service is not running here). Client impact
cross-checked against the Unity explorer EventDTO
(`decentraland/unity-explorer/Explorer/Assets/DCL/EventsApi/EventDTO.cs`)
and the net-catalog (`the Unity net-catalog`).

## Method note — what the explorer actually consumes

The decisive skeptic filter for every shape diff was the Unity client's wire
struct `EventDTO`. The explorer deserializes **only** these event fields:

```
id, name, image, description, next_start_at, next_finish_at, finish_at,
scene_name, coordinates, server, total_attendees, live, user_name,
highlighted, trending, attending, categories, recurrent, duration, start_at,
recurrent_dates, world, x, y, place_id, connected_addresses, community_id,
image_vertical
```

Per the net-catalog, the explorer calls only: `GET /api/events` (several query
shapes, incl. `with_connected_users={bool}`), `POST /api/events/search`,
`POST /api/events/{id}/attendees`, `DELETE /api/events/{id}/attendees`. It does
**not** call `GET /api/events/{id}`, `GET /api/events/{id}/attendees`,
`/api/events/categories`, `/api/schedules*`, `/api/poster*`,
`/api/profiles/*/settings`, `/api/profiles/subscriptions`,
`GET /api/events/attending`, or any sitemap route. Severity below is graded for
the **explorer**; web-frontend impact is noted where it diverges.

## Per-endpoint table

| Endpoint | Shape | Efficiency | Severity | Notes |
|---|---|---|---|---|
| GET /ping | match | same | none | Local liveness echo; no upstream route. |
| GET /api/events | divergent | **worse (corrected)** | major | Explorer-facing. Drops `connected_addresses` and `scene_name` (both read by the client). Count query runs unconditionally → 1 extra `count(*)` on the common bare-list path. ILIKE vs ts_rank_cd; `finish_at` window vs `next_finish_at`. |
| POST /api/events | divergent | n/a | minor (explorer) | 501. Explorer never creates events (uses web deep-link). Web-frontend break. |
| POST /api/events/search | divergent | same | major | Explorer-facing (community events by placeIds). Same item-shape gaps as GET. Envelope `{events,total}` correct. |
| GET /api/events/attending | divergent | n/a | minor (explorer) | 501. Explorer does not call it. |
| GET /api/events/categories | divergent | better | minor | Explorer does not call it. Static 15-cat list, missing created_at/updated_at, drift risk vs DB. |
| GET /api/events/{event_id} | divergent | better | minor (explorer) | Explorer does not call single-event GET. Avoids the auth `EventAttendee.count`; visibility/attending diverge. |
| PATCH /api/events/{event_id} | divergent | n/a | minor (explorer) | 501 moderation. Web/admin only. |
| GET /api/events/{event_id}/attendees | divergent | better | minor (explorer) | Explorer does not call it. Capped at 10, synthetic user_name/created_at. |
| POST /api/events/{event_id}/attendees | divergent | n/a | **major** | 501. Explorer RSVP-join — real client break. |
| DELETE /api/events/{event_id}/attendees | divergent | n/a | **major** | 501. Explorer RSVP-cancel — real client break. |
| GET /api/schedules | divergent | degenerate-stub | minor (explorer) | Always `[]`. Explorer does not call it. Web break. |
| GET /api/schedules/{schedule_id} | divergent | degenerate-stub | minor (explorer) | Always 404. Web break. |
| POST/PATCH /api/schedules | divergent | n/a | minor (explorer) | 501 admin-only. |
| POST /api/poster[-vertical] | divergent | n/a | minor (explorer) | 501, no S3. Web upload break. |
| GET/PATCH /api/profiles*/settings (5) | divergent | n/a | minor (explorer) | 501. Explorer does not call them. |
| GET/POST/DELETE /api/profiles/subscriptions | divergent | n/a | minor | 501. Upstream `@deprecated` (web-push). |
| GET /events/sitemap*.xml (4) | divergent | n/a | minor | 501 text/plain. Crawler-only, excluded by design. |
| GET /federation/v1/events/* | n/a (native) | n/a | none | catalyrst-native gossip stubs; no upstream. |

## Confirmed shape issues (real AND explorer-relevant)

1. **`connected_addresses` dropped on GET /api/events and POST /api/events/search.**
   - Rust: `EventRecord` (schemas.rs:5-51) has no such field; `EventListQuery`
     parses `with_connected_users` (events.rs:33) but the list path never emits it.
   - Upstream adds it per-event when `with_connected_users=true`
     (getEventList.ts:101-117, 308-314), keyed by world server or `x,y`.
   - Client impact: `EventCardView.cs:170` computes
     `onlineMembers = eventInfo.connected_addresses?.Length ?? 0` and
     `EventsStateService.cs:41-43` iterates it. The explorer requests
     `with_connected_users={bool}` on highlighted and date-range listings
     (net-catalog). With our impl the "online members" count is always **0**.
     Severity: major (silent wrong UI count on event cards).

2. **`scene_name` dropped on every event item.**
   - Rust: no `scene_name` field anywhere in `EventRecord`. `estate_name` is
     emitted but upstream's `estate_name` is `event.estate_name || event.scene_name`
     (model.ts:483); the explorer reads `scene_name` directly.
   - Client impact: `EventDetailPanelView.cs:122` renders
     `placeNameText.text = $"{eventData.Scene_name} ({eventData.X},{eventData.Y})"`.
     Missing → the venue line shows an empty name. Severity: major for the detail
     panel (explorer reads it via `EventDTO.scene_name` and `EventWithPlaceIdDTO`).

3. **Attendee list truncated + synthetic (GET /api/events/{id}/attendees).**
   - Rust: derived from cached `latest_attendees` jsonb (ports/attendees.rs:17-42);
     `user_name` always `None`, `created_at` always `now()`, capped at the
     `LATEST_EVENT_ATTENDING=10` cache size.
   - Upstream: real `SELECT event_id,user,user_name,created_at ... ORDER BY
     created_at DESC LIMIT 500` (EventAttendee/model.ts:27-34).
   - Real content/correctness loss, but the **explorer never calls this GET** (only
     POST/DELETE), so explorer severity is low; matters for web/integrations.

4. **Schedules serve no data.** `GET /api/schedules` always `[]`,
   `GET /api/schedules/{id}` always 404 (ports/schedules.rs:15-21). Upstream
   returns active schedules (`active_until > now`, model.ts:18) and resolves by id.
   Shape would match (`ScheduleRecord` == `ScheduleAttributes`, incl.
   created_at/updated_at) but the stub is degenerate. Explorer does not call it.

5. **Categories: static 15-item list, no created_at/updated_at, drift risk.**
   - Rust: `STATIC_CATEGORIES` (ports/categories.rs:11-27); `EventCategoryRecord`
     omits created_at/updated_at present in upstream `...category` spread
     (EventCategory/routes.ts:24-29, types.ts:1-6). Ordering not guaranteed to
     match DB `name asc`. Explorer does not call this endpoint and the per-event
     `categories` array (which the client DOES read) is unaffected. Low explorer impact.

6. **Semantic list divergences (GET /api/events & search):** active window uses
   `finish_at >= now` (ports/events.rs:81) vs upstream `next_finish_at`; search is
   `ILIKE` on name/description (ports/events.rs:126-129) vs upstream `ts_rank_cd`
   textsearch; ordering `COALESCE(start_at,finish_at)` (ports/events.rs:148) vs
   upstream next_start/next_finish. Result set and ordering can differ for the same
   query — affects the explorer (it is the live list path), severity minor (no field
   break, but visible ordering/recall differences).

7. **501 writes that the explorer actually calls:** `POST` and `DELETE
   /api/events/{id}/attendees` (attendees.rs:17-27) are the RSVP join/cancel and are
   invoked by `HttpEventsApiService.MarkAsInterestedAsync/MarkAsNotInterestedAsync`.
   These are hard failures for the explorer today — major (deferred to the
   federation write path by design).

## Confirmed efficiency wins (with structural reason)

- **GET /api/events/{event_id}: fewer queries (better).** Single
  `SELECT ... WHERE id=$1` (ports/events.rs:184-194). Upstream does `findOne` plus a
  second `EventAttendee.count` whenever the request is authed (getEvent.ts:81-89).
  Structural, not language-based — ours genuinely omits the second round-trip (at the
  cost of correct `attending` and owner/admin pending-visibility).
- **GET /api/events/{id}/attendees: 1 query vs 2 (better, but degraded).** Ours reads
  the event's cached `latest_attendees` in one `SELECT` (ports/attendees.rs:17);
  upstream does `event findOne` + `event_attendees` list (2 SELECTs). Genuinely fewer
  queries, but only because it serves a truncated/synthetic list — correctness traded
  for the query.
- **GET /api/events/categories: no DB vs 1 SELECT (better).** Static slice
  (ports/categories.rs); upstream `SELECT ... WHERE active=true ORDER BY name asc`
  per request, uncached. Fewer queries for an effectively static dataset — but drift
  risk and missing timestamps.

## Rejected / corrected during verification

- **REJECTED (explorer-impact) — "MISSING created_at, updated_at, contact, details,
  approved_by, rejected_by, rejection_reason" on event items.** These fields are
  genuinely absent from `EventRecord` and genuinely present in upstream `toPublic`
  (`...event` spread; contact/details are owner-only, model.ts:466-468). BUT the Unity
  explorer's `EventDTO` does not declare any of them, so it ignores them entirely.
  For the explorer these are **cosmetic**, not functional. They remain real gaps for
  the web frontend / API integrators; keep them as "web-only" notes, not explorer
  severity. (scene_name, which the original finding lumped into this list, is the
  exception — the client DOES read it — and is promoted to a confirmed major above.)

- **CORRECTED — "GET /api/events efficiency: same."** Our `list()` runs the
  `count(*) FROM (subquery)` (ports/events.rs:143,167-178) **unconditionally** on every
  call; the handler only uses `total` in the `{events,total}` envelope branch
  (events.rs:125-131). Upstream runs `countEventsWithFilter` **only** inside the
  places_ids/community_id branch (getEventList.ts:317-321). So on the common bare-list
  path (`GET /api/events?list=live`, `list=highlight`, etc. with no places_ids/
  community_id) ours issues an **extra** count query upstream skips → marginally
  **worse**, not "same." (Offsetting note: ours avoids upstream's N-parallel
  comms-gatekeeper fan-out, but only by dropping `connected_addresses`, which is a
  shape regression, not an efficiency win.) This is a cheap fix: guard the count
  behind `envelope_with_total`.

- **CONFIRMED not a regression — envelope and snake_case item keys.** Bare
  `{ok:true,data:[...]}` vs `{ok:true,data:{events,total}}` switching on
  places_ids/community_id (events.rs:125-131) matches upstream exactly; error envelope
  `{ok:false,error}` (http/response.rs:67) matches `RequestError`. Default serde
  snake_case == upstream DB column names; no rename mismatch. The `placeIds`/
  `communityId` body rename (events.rs:136-139) matches upstream camelCase body keys.
