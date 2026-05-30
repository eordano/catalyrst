# catalyrst-events route inventory

Source: a read-only mirror of the upstream `events` service `src/` tree.

All upstream routes are mounted under `/api/*` except the sitemap (`/events/sitemap*.xml`).
Every JSON response is wrapped `{ok: true, data: <T>}` (success) or
`{ok: false, error: ...}` (failure) by the `decentraland-gatsby` `handle()` helper.

## Summary

- **Total upstream routes:** 28
- **Ported as GET (parity-target):** 8
- **Stubbed (501) writes / federation actions:** 16
- **Out of scope (S3 upload / notification subscription / sitemap):** 4

## Routes

### `Event/` (`src/entities/Event/routes/index.ts`)

| Method | Path                          | Upstream handler             | catalyrst-events status |
|--------|-------------------------------|------------------------------|-------------------------|
| GET    | `/api/events`                 | `getEventList`               | ported (read)           |
| POST   | `/api/events`                 | `createEvent`                | 501 (federation write)  |
| POST   | `/api/events/search`          | `getEventList` (body filter) | ported (read; POST body) |
| GET    | `/api/events/attending`       | `getAttendingEventList`      | 501 (needs auth)        |
| GET    | `/api/events/{event_id}`      | `getEventWithOptions`        | ported (read)           |
| PATCH  | `/api/events/{event_id}`      | `patchEventRoute`            | 501 (federation write)  |

### `EventAttendee/` (`src/entities/EventAttendee/routes.ts`)

| Method | Path                                         | Upstream handler        | catalyrst-events status |
|--------|----------------------------------------------|-------------------------|-------------------------|
| GET    | `/api/events/{event_id}/attendees`           | `getEventAttendees`     | ported (read)           |
| POST   | `/api/events/{event_id}/attendees`           | `createEventAttendee`   | 501 (federation write)  |
| DELETE | `/api/events/{event_id}/attendees`           | `deleteEventAttendee`   | 501 (federation write)  |

### `EventCategory/` (`src/entities/EventCategory/routes.ts`)

| Method | Path                       | Upstream handler         | catalyrst-events status |
|--------|----------------------------|--------------------------|-------------------------|
| GET    | `/api/events/categories`   | `getEventCategoryList`   | ported (static stub)    |

### `Schedule/` (`src/entities/Schedule/routes.ts`)

| Method | Path                              | Upstream handler  | catalyrst-events status |
|--------|-----------------------------------|-------------------|-------------------------|
| GET    | `/api/schedules`                  | `getScheduleList` | ported (read)           |
| GET    | `/api/schedules/{schedule_id}`    | `getScheduleById` | ported (read; 404 stub) |
| POST   | `/api/schedules`                  | `createSchedule`  | 501 (federation write)  |
| PATCH  | `/api/schedules/{schedule_id}`    | `updateSchedule`  | 501 (federation write)  |

### `Poster/` (`src/entities/Poster/routes.ts`)

| Method | Path                          | Upstream handler           | catalyrst-events status |
|--------|-------------------------------|----------------------------|-------------------------|
| POST   | `/api/poster`                 | `uploadPoster`             | 501 (S3 out of scope)   |
| POST   | `/api/poster-vertical`        | `uploadPosterVertical`     | 501 (S3 out of scope)   |

### `ProfileSettings/` (`src/entities/ProfileSettings/routes.ts`)

| Method | Path                                          | Upstream handler            | catalyrst-events status |
|--------|-----------------------------------------------|-----------------------------|-------------------------|
| GET    | `/api/profiles/settings`                      | `listProfileSettings`       | 501 (admin only)        |
| GET    | `/api/profiles/me/settings`                   | `getAuthProfileSettings`    | 501 (auth-chain)        |
| PATCH  | `/api/profiles/me/settings`                   | `updateMyProfileSettings`   | 501 (federation write)  |
| GET    | `/api/profiles/{profile_id}/settings`         | `getProfileSettings`        | 501 (admin only)        |
| PATCH  | `/api/profiles/{profile_id}/settings`         | `updateProfileSettings`     | 501 (federation write)  |

### `ProfileSubscription/` (`src/entities/ProfileSubscription/routes.ts`) — `@deprecated` upstream

| Method | Path                              | Upstream handler             | catalyrst-events status |
|--------|-----------------------------------|------------------------------|-------------------------|
| GET    | `/api/profiles/subscriptions`     | `getProfileSubscription`     | 501 (deprecated upstream) |
| POST   | `/api/profiles/subscriptions`     | `createProfileSubscription`  | 501 (deprecated upstream) |
| DELETE | `/api/profiles/subscriptions`     | `removeProfileSubscription`  | 501 (deprecated upstream) |

### `Sitemap/` (`src/entities/Sitemap/routes.ts`)

| Method | Path                              | Upstream handler        | catalyrst-events status |
|--------|-----------------------------------|-------------------------|-------------------------|
| GET    | `/events/sitemap.xml`             | `getIndexSitemap`       | out of scope            |
| GET    | `/events/sitemap.static.xml`      | `getStaticSitemap`      | out of scope            |
| GET    | `/events/sitemap.events.xml`      | `getEventsSitemap`      | out of scope            |
| GET    | `/events/sitemap.schedules.xml`   | `getSchedulesSitemap`   | out of scope            |

### Federation snapshot endpoints (per `docs/federation/events.md` §4)

| Method | Path                                              | Status      |
|--------|---------------------------------------------------|-------------|
| GET    | `/federation/v1/events/feed`                      | stub (returns `[]`) |
| GET    | `/federation/v1/events/{event_id}/attendance`     | stub (returns `[]`) |

### Health

| Method | Path     | Status                       |
|--------|----------|------------------------------|
| GET    | `/ping`  | ported (returns `/ping`)     |
