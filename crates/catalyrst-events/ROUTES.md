# catalyrst-events route inventory

Source: a read-only mirror of the upstream `events` service `src/` tree.
All upstream routes mount under `/api/*` except the sitemap
(`/events/sitemap*.xml`). Every JSON response is wrapped
`{ok: true, data: <T>}` (success) or `{ok: false, error: ...}` (failure) by
the `decentraland-gatsby` `handle()` helper.

Summary: 29 upstream routes total (includes `DELETE /api/events/{event_id}`).
Ported as GET (parity-target): 11; as writes (federation-envelope or
admin/signer-gated): 10; deprecated upstream (410 Gone): 3
(`ProfileSubscription/*`); still 501: 1 (`DELETE /api/events/{event_id}`);
sitemap: 4. Local addition (not upstream): 1 (`GET /api/events/moderation`).

### `Event/` (`src/entities/Event/routes/index.ts`)

| Method | Path                          | Upstream handler             | catalyrst-events status |
|--------|-------------------------------|------------------------------|-------------------------|
| GET    | `/api/events`                 | `getEventList`               | ported (read)           |
| POST   | `/api/events`                 | `createEvent`                | ported (write; admin-gated via `admin::authorize_admin`) |
| POST   | `/api/events/search`          | `getEventList` (body filter) | ported (read; POST body) |
| GET    | `/api/events/attending`       | `getAttendingEventList`      | ported (read; requires auth) |
| GET    | `/api/events/{event_id}`      | `getEventWithOptions`        | ported (read)           |
| PATCH  | `/api/events/{event_id}`      | `patchEventRoute`            | ported (write; admin-gated moderation actions) |
| DELETE | `/api/events/{event_id}`      | -                             | 501 (not implemented; "event deletion is handled via the federation write path") |
| GET    | `/api/events/moderation`      | -                             | local addition (not in upstream; admin-gated moderation queue) |

### `EventAttendee/` (`src/entities/EventAttendee/routes.ts`)

| Method | Path                                         | Upstream handler        | catalyrst-events status |
|--------|----------------------------------------------|-------------------------|-------------------------|
| GET    | `/api/events/{event_id}/attendees`           | `getEventAttendees`     | ported (read)           |
| POST   | `/api/events/{event_id}/attendees`           | `createEventAttendee`   | ported (write; signer-gated RSVP) |
| DELETE | `/api/events/{event_id}/attendees`           | `deleteEventAttendee`   | ported (write; signer-gated RSVP) |

### `EventCategory/` (`src/entities/EventCategory/routes.ts`)

| Method | Path                       | Upstream handler         | catalyrst-events status |
|--------|----------------------------|--------------------------|-------------------------|
| GET    | `/api/events/categories`   | `getEventCategoryList`   | ported (static stub)    |

### `Schedule/` (`src/entities/Schedule/routes.ts`)

| Method | Path                              | Upstream handler  | catalyrst-events status |
|--------|-----------------------------------|-------------------|-------------------------|
| GET    | `/api/schedules`                  | `getScheduleList` | ported (read)           |
| GET    | `/api/schedules/{schedule_id}`    | `getScheduleById` | ported (read; 404 stub) |
| POST   | `/api/schedules`                  | `createSchedule`  | ported (write; federation envelope, `handlers/schedules.rs::apply_upsert`) |
| PATCH  | `/api/schedules/{schedule_id}`    | `updateSchedule`  | ported (write; federation envelope, `handlers/schedules.rs::apply_upsert`) |

### `Poster/` (`src/entities/Poster/routes.ts`)

| Method | Path                          | Upstream handler           | catalyrst-events status |
|--------|-------------------------------|----------------------------|-------------------------|
| POST   | `/api/poster`                 | `uploadPoster`             | ported (write; signer-gated multipart upload to local `content_store`, not S3) |
| POST   | `/api/poster-vertical`        | `uploadPosterVertical`     | ported (write; signer-gated multipart upload to local `content_store`, not S3) |

### `ProfileSettings/` (`src/entities/ProfileSettings/routes.ts`)

| Method | Path                                          | Upstream handler            | catalyrst-events status |
|--------|-----------------------------------------------|-----------------------------|-------------------------|
| GET    | `/api/profiles/settings`                      | `listProfileSettings`       | ported (read; moderator-gated via `fed::authority::require_moderator`) |
| GET    | `/api/profiles/me/settings`                   | `getAuthProfileSettings`    | ported (read; auth-chain gated) |
| PATCH  | `/api/profiles/me/settings`                   | `updateMyProfileSettings`   | ported (write; federation envelope, self-only) |
| GET    | `/api/profiles/{profile_id}/settings`         | `getProfileSettings`        | ported (read; moderator-gated) |
| PATCH  | `/api/profiles/{profile_id}/settings`         | `updateProfileSettings`     | ported (write; federation envelope) |

### `ProfileSubscription/` (`src/entities/ProfileSubscription/routes.ts`) - `@deprecated` upstream

| Method | Path                              | Upstream handler             | catalyrst-events status |
|--------|-----------------------------------|------------------------------|-------------------------|
| GET    | `/api/profiles/subscriptions`     | `getProfileSubscription`     | 410 Gone (deprecated upstream) |
| POST   | `/api/profiles/subscriptions`     | `createProfileSubscription`  | 410 Gone (deprecated upstream) |
| DELETE | `/api/profiles/subscriptions`     | `removeProfileSubscription`  | 410 Gone (deprecated upstream) |

### `Sitemap/` (`src/entities/Sitemap/routes.ts`)

| Method | Path                              | Upstream handler        | catalyrst-events status |
|--------|-----------------------------------|-------------------------|-------------------------|
| GET    | `/events/sitemap.xml`             | `getIndexSitemap`       | ported (`handlers/sitemap.rs::sitemap_index`) |
| GET    | `/events/sitemap.static.xml`      | `getStaticSitemap`      | ported (`handlers/sitemap.rs::sitemap_static`) |
| GET    | `/events/sitemap.events.xml`      | `getEventsSitemap`      | ported (`handlers/sitemap.rs::sitemap_events`) |
| GET    | `/events/sitemap.schedules.xml`   | `getSchedulesSitemap`   | ported (`handlers/sitemap.rs::sitemap_schedules`) |

### Federation snapshot endpoints (per `docs/federation/events.md` section 4)

| Method | Path                                              | Status      |
|--------|---------------------------------------------------|-------------|
| GET    | `/federation/v1/events/feed`                      | stub (returns `[]`) |
| GET    | `/federation/v1/events/{event_id}/attendance`     | stub (returns `[]`) |

### Health

| Method | Path     | Status                       |
|--------|----------|------------------------------|
| GET    | `/ping`  | ported (returns `/ping`)     |
