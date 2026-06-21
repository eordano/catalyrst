# catalyrst-events TODO — federation-writes phase

This crate ports the **read path** of `decentraland/events`. Writes
(create event, attend, moderate, profile-settings update) all return `501
Not Implemented` with a pointer to `docs/federation/events.md`. This file
lists what the federation-writes agent needs to wire up.

## Federation phase wiring

The signed-action types are already defined in [`src/signed.rs`](src/signed.rs):

- `EventCreate` — wraps title/description/start_at/end_at/location, `TypedMessage::PRIMARY_TYPE = "EventCreate"`
- `EventAttend` — `{event_id, action: attend|cancel, signed_at}`
- `EventModerate` — `{event_id, action: ban|hide|feature, signed_at}`

All three impl `catalyrst_fed::sig::TypedMessage`. The encode-struct
implementations are minimal byte-concat layouts; if the federation team
adopts a richer canonical encoding (RLP, canonical-JSON, etc.) update those
three impls — no other crate code touches the encoding.

`Signed<EventCreate>` lives in [`src/federation.rs`](src/federation.rs) as
the `/federation/v1/events/feed` response element. The local store backing
those endpoints is the missing piece: a new table inside `places_events`
(or a dedicated `events_local` per ADR §2) holding `Signed<T>` payloads as
canonical JSON. Once that exists:

1. Replace the empty-`Vec` returns in
   [`src/handlers/federation.rs`](src/handlers/federation.rs) with real
   reads from `events_local` / `event_attendance_local`.
2. Wire `POST /api/events`, `POST /api/events/{id}/attendees`, and
   `DELETE /api/events/{id}/attendees` in
   [`src/handlers/events.rs`](src/handlers/events.rs) and
   [`src/handlers/attendees.rs`](src/handlers/attendees.rs) to:
   - parse the `Signed<EventCreate>` / `Signed<EventAttend>` body,
   - call `signed.verify(signed.signer()?.as_str(), now)` from
     `catalyrst_fed::Signed::verify`,
   - rate-limit per `signed.signer()` per ADR §5 via
     `catalyrst_fed::limits::RateLimiter`,
   - insert into the local store.
3. `PATCH /api/events/{id}` follows the same shape with `EventModerate`,
   gated on the local `moderators` allow-list per ADR §3.

## Schema divergence (places_events archive → upstream API shape)

The `places_events.event` table is an **archive** of the live
foundation API, not the foundation's source-of-truth schema. Where columns
diverge, we extract from the JSONB `raw` column:

| Upstream field | Archive column | Status in this port |
|---|---|---|
| `id`, `name`, `start_at`, `finish_at`, `duration`, `recurrent`, `highlighted`, `trending`, `approved`, `community_id`, `user_creator`, `coordinates_x`, `coordinates_y`, `description` | top-level | ported direct |
| `image`, `image_vertical`, `server`, `url`, `user`, `user_name`, `estate_id`, `estate_name`, `world`, `all_day`, `next_start_at`, `next_finish_at`, `recurrent_*`, `categories`, `schedules`, `latest_attendees`, `total_attendees`, `place_id`, `rejected`, `rejection_reason`, `approved_by`, `rejected_by` | `raw->>...` | ported via JSON extraction |
| `attending` (per-request) | not stored | always `false` until federation auth-chain |
| `connected_addresses` (comms gatekeeper) | not stored | not ported; needs CommsGatekeeper client port (see `api/CommsGatekeeper.ts`) |

## Top 3 schema divergences to fix before parity gate

1. **`schedules` array.** Archive doesn't ingest the join from `event_schedule`
   (the upstream junction table). Today we read `raw->>'schedules'` which is
   sometimes present and sometimes empty. The `/api/schedules` endpoint
   itself returns `[]` because the archive doesn't materialise the
   `schedule` table at all. To fix, either:
   - extend `events-archive.py` to ingest `/api/schedules` daily, OR
   - federate schedule lifecycle as a `Signed<ScheduleAction>` action class
     (new addition to `docs/federation/events.md`).

2. **`textsearch` / `search=` query parameter.** Upstream uses Postgres
   `tsvector` with weighted columns (`name='A'`, `user_name+estate_name='B'`,
   `description='C'`). The archive doesn't have a `textsearch` column; this
   port falls back to `ILIKE %s%` over `name` and `description`. Searches
   for estate names or user names return fewer results than upstream. To
   fix, add a generated `tsvector` column to `places_events.event` in
   `bootstrap-places-events.sh` and switch the port to `websearch_to_tsquery`.

3. **`only_attendee=true` + `/api/events/attending`.** Both depend on the
   `event_attendee` junction table, which the archive doesn't have at all
   (the archive only stores `raw->>'latest_attendees'`, a denormalised
   front-page sample). Both endpoints return 501 today, even though
   `/api/events/attending` is a GET. They'll come online with the
   federation phase, when `event_attendance_local` exists.

## Lesser divergences

- `EventCategory` is served from a 15-item static list inside
  [`src/ports/categories.rs`](src/ports/categories.rs) because the archive
  doesn't ingest `/api/events/categories`. If the upstream category list
  expands, update the static list. The static list matches the live API
  as of 2026-05.
- `/api/events/{id}` returns 404 for `approved=false` events (rejected,
  pending, hidden). Upstream returns the event if the requesting user is
  an admin or the event author. We can't make that distinction without
  auth-chain; admin/author lookups land with the federation phase.
- The `with_connected_users=true` flag is silently ignored. It would
  require a `CommsGatekeeper` client port (`api/CommsGatekeeper.ts`) which
  is out of scope for this port — it's an operational concern of the
  catalyst, not part of the events API contract.
- `created_at` / `updated_at` on events: archive stores `fetched_at`
  (when the archive scraped the row), not the upstream event's
  `created_at` / `updated_at`. Port reads `raw->>'created_at'` etc. so
  the API shape is correct; for events that landed in the archive before
  the archive started preserving `created_at`, the field will be `null`.

## Out-of-scope routes (stubbed as 501)

- `POST /api/poster`, `POST /api/poster-vertical` — S3-backed upload. Not
  provisioned for catalysts; if a catalyst wants its own poster bucket it
  ships its own image service and rewrites at nginx.
- `/events/sitemap*.xml` — crawler endpoints owned by the foundation;
  per-catalyst sitemap is meaningless under federation.
- `/api/profiles/subscriptions` (GET / POST / DELETE) — already
  `@deprecated` upstream (web-push subscriptions, replaced by the
  centralised notification service); not ported.
- `/api/profiles/{id}/settings` and `/api/profiles/me/settings` — these
  are per-user notification preferences (email/push opt-in). The federation
  ADR doesn't model notification routing; the per-catalyst notification path
  is the operator's choice.

## Tests not yet ported

The upstream Jest integration suite at `test/integration/` runs against a
local Postgres. Until the federation-writes phase, this port has no DB
fixtures to test against (the archive is a moving target). A future
`crates/catalyrst-events/tests/` directory should mirror the upstream's
`test/integration/listEvents.test.ts`, `getEvent.test.ts`, etc., backed by
a `sqlx`-managed local `events_local` schema.
