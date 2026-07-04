# catalyrst-events TODO — federation-writes phase

**Writes are implemented as of 2026-07-03 (docs-stale-audit).** This section
previously claimed every write (create event, attend, moderate,
profile-settings update) returns `501 Not Implemented` pointing at
`docs/federation/events.md`. That file does not exist anywhere in the repo
(`docs/federation/` only holds `gossip-runbook.md`), and the claim itself is
false: `src/handlers/{events,attendees,schedules,profile_settings}.rs` all
have real implementations today. The only route in this crate that still
returns 501 is `DELETE /api/events/{event_id}` (see `ROUTES.md`).

What's actually wired, split by write path:

- **Local writes (no cross-peer gossip):** `POST /api/events`,
  `PATCH /api/events/{id}` (`handlers/events.rs::create_event`/`patch_event`,
  gated on `admin::authorize_admin` — an admin bearer token, not a signed
  peer envelope) and `POST`/`DELETE /api/events/{id}/attendees`
  (`handlers/attendees.rs`, gated on the local `auth_chain::require_signer`
  dcl-crypto signed-fetch check). These persist locally but do **not** call
  `emit_gossip` — they aren't propagated to federation peers today.
- **Federated writes (Signed<T> envelope + gossip):** `POST`/`PATCH
  /api/schedules*` (`handlers/schedules.rs::apply_upsert`, `ScheduleUpsert`
  message type) and `PATCH /api/profiles/{me,{id}}/settings`
  (`handlers/profile_settings.rs`, `ProfileSettingsUpdate` message type) both
  go through `handlers/federation.rs::preflight::<T>` (parses `Signed<T>`,
  verifies signature, replay-checks via `fed::replay`) and `emit_gossip` on
  success.

Storage: `signed_actions_events`, `moderators`, `event_profile_settings`,
`schedules_local`, `seen_nonces` — all created by the real migration
`migrations/0001_federation.sql` (contrary to earlier drafts of this file,
this crate does have committed SQL migrations, unlike `catalyrst-places`
which creates its tables inline). **DONE.**

The signed-action types (`src/fed/messages.rs`, not `src/signed.rs` — that
path never existed) include `EventCreate`, `EventAttend`, and
`EventModerate` in addition to the two actually wired above
(`ProfileSettingsUpdate`, `ScheduleUpsert`). `EventCreate`/`EventAttend`/
`EventModerate` are defined and impl `TypedMessage` but are **not**
referenced anywhere in `src/handlers/*.rs` — they're dead/reserved code, not
the mechanism `create_event`/attendee RSVP actually use today.

## Open: extend gossip to events + attendees

Event create/patch/delete and attendee RSVP are real, working, local writes
today, but — unlike schedules and profile-settings — they don't propagate to
federation peers. To close that gap:

1. Decide whether to route `create_event`/`patch_event`/attendee RSVP
   through the existing `Signed<EventCreate>`/`Signed<EventAttend>`/
   `Signed<EventModerate>` types (already defined, unused) via
   `handlers/federation.rs::preflight`, or keep them as catalyst-local admin
   actions by design.
2. If federated: call `emit_gossip` on success. A gossip consumer already
   exists (`src/fed/consumer.rs::spawn`/`run`, subscribed on
   `Scope::Events`) but today only handles `ProfileSettingsUpdate` and
   `ScheduleUpsert` envelopes — it would need `EventCreate`/`EventAttend`/
   `EventModerate` arms added.
3. Implement `DELETE /api/events/{event_id}` (`handlers/events.rs:602`,
   still `ApiError::not_implemented`) — the only remaining 501 in this
   crate.

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

3. **`only_attendee=true` + `/api/events/attending`. DONE (2026-07-03).**
   Both are implemented and no longer return 501. A local
   `event_attendance_local` table (populated by the attendee RSVP writes,
   see the federation-phase section above) now backs both: `only_attendee`
   filters via `id IN (SELECT event_id FROM event_attendance_local WHERE
   signer = ... AND action = 'going') OR raw->'latest_attendees' ? ...`, and
   `GET /api/events/attending` (`ports/events.rs::attending`) does a real
   join against it, falling back to the archive's denormalised
   `raw->>'latest_attendees'` sample.

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
- `with_connected_users=true` (upstream events@b17e17a) is implemented:
  [`src/clients.rs`](src/clients.rs) ports `api/CommsGatekeeper.ts` against
  the local comms/archipelago occupancy service (`catalyrst-comms`,
  `GET /scene-participants`, `COMMS_GATEKEEPER_URL`, default `:5138`). The
  handler attaches the LIVE connected wallets per event location (world name
  for worlds, `"x,y"` pointer for scenes), cached 5 min; a comms outage
  degrades to an empty roster.
- `created_at` / `updated_at` on events: archive stores `fetched_at`
  (when the archive scraped the row), not the upstream event's
  `created_at` / `updated_at`. Port reads `raw->>'created_at'` etc. so
  the API shape is correct; for events that landed in the archive before
  the archive started preserving `created_at`, the field will be `null`.

## Corrections to this section (2026-07-03 docs-stale-audit)

The routes below were previously listed as "out of scope, stubbed as 501."
That was true for `/api/profiles/subscriptions` but false for the rest —
`POST /api/poster`/`poster-vertical`, the sitemap routes, and both
profile-settings route families are all implemented and ported:

- `POST /api/poster`, `POST /api/poster-vertical` — **ported, not
  S3-backed.** `handlers/poster.rs` accepts a signer-gated multipart upload
  and stores it content-addressed in a local `content_store` (backed by
  `content_dir`), returning the same `{filename, url, size, type}` shape as
  upstream. No S3 dependency exists in this crate.
- `/events/sitemap*.xml` — **ported.** `handlers/sitemap.rs` generates all
  four sitemap XML documents from the local `event`/`schedules_local` data
  (`sitemap_index`, `sitemap_static`, `sitemap_events`, `sitemap_schedules`).
- `/api/profiles/subscriptions` (GET / POST / DELETE) — still correctly
  not ported: already `@deprecated` upstream (web-push subscriptions,
  replaced by the centralised notification service). This one returns
  `410 Gone`, not `501` (see `ROUTES.md`).
- `/api/profiles/{id}/settings` and `/api/profiles/me/settings` — **ported,
  federation-writes phase.** See the federation phase section above:
  `PATCH` on both goes through the `Signed<ProfileSettingsUpdate>` envelope
  + gossip; `GET` is moderator/self-gated and reads real data.

## Tests — DONE (2026-07-03), upstream Jest suite still not mirrored 1:1

`crates/catalyrst-events/tests/federation.rs` (446 lines) already exists and
runs against a real (schema-isolated) Postgres: it covers the moderator
gate + schedule + profile-settings federation flow
(`moderator_gate_and_schedule_and_settings_flow`), soft-deleted events being
hidden from all reads, and owner-filtered status visibility. This is not a
line-for-line port of the upstream Jest `test/integration/*.test.ts` suite
(`listEvents.test.ts`, `getEvent.test.ts`, etc.) — that 1:1 mirroring is
still open — but the earlier claim that "this port has no DB fixtures to
test against" is no longer true.

> _Re-verified against code 2026-07-03 (docs-stale-audit); corrections applied._
