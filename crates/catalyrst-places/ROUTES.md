# catalyrst-places — Route inventory

Upstream source: `decentraland/places` (`src/server.ts` mounts each entity router under
`/api`, and `socialRoutes` separately under `/places`). Reproduced verbatim below
with method + path as the upstream registers them, the upstream handler file, and
this port's status.

Status legend:
- `GET`  — ported, reads from the archive `places_events` DB
- `STUB` — read endpoint that depends on cross-service surfaces not present in
           the archive (`place_categories`, `place_positions`, `worlds`,
           `destinations`, `hot_scenes`, etc.); returns the upstream
           empty-shape envelope and is documented in TODO.md
- `FED`  — write endpoint, implemented: verifies a `Signed<T>` federation
           envelope (`handlers/federation.rs`), replay-checks it, persists
           locally, and gossips to peers
- `ADMIN` — write endpoint gated on `require_admin_bearer` (`handlers/admin.rs`),
           not part of upstream `decentraland/places`

## Mounted under `/api/`

| Method | Path                                  | Upstream file                                              | Status |
|--------|---------------------------------------|------------------------------------------------------------|--------|
| GET    | `/api/categories`                     | `entities/Category/routes.ts`                              | STUB   |
| PATCH  | `/api/places/:entity_id/favorites`    | `entities/UserFavorite/routes.ts`                          | FED    |
| PATCH  | `/api/places/:entity_id/likes`        | `entities/UserLikes/routes.ts`                             | FED    |
| GET    | `/api/places/:place_id`               | `entities/Place/routes/getPlace.ts`                        | GET    |
| GET    | `/api/places`                         | `entities/Place/routes/getPlaceList.ts`                    | GET    |
| POST   | `/api/places`                         | `entities/Place/routes/getPlaceListById.ts`                | GET    |
| PUT    | `/api/places/:place_id/rating`        | `entities/Place/routes/updateRating.ts`                    | FED    |
| PUT    | `/api/places/:place_id/ranking`       | `entities/Place/routes/updateRanking.ts`                   | FED    |
| PUT    | `/api/places/:place_id/highlight`     | `entities/Place/routes/updateHighlight.ts`                 | FED    |
| GET    | `/api/places/:place_id/categories`    | `entities/Place/routes/getPlaceCategories.ts`              | STUB   |
| PUT    | `/api/places/:place_id/featured`      | `entities/Place/routes/featured.ts` (`featurePlace`)       | FED    |
| DELETE | `/api/places/:place_id/featured`      | `entities/Place/routes/featured.ts` (`unfeaturePlace`)     | FED    |
| POST   | `/api/places/status`                  | `entities/Place/routes/getPlaceStatusListById.ts`          | GET    |
| GET    | `/api/worlds/:world_id`               | `entities/World/routes/getWorld.ts`                        | STUB   |
| GET    | `/api/worlds`                         | `entities/World/routes/getWorldList.ts`                    | STUB   |
| GET    | `/api/world_names`                    | `entities/World/routes/getWorldNamesList.ts`               | STUB   |
| PATCH  | `/api/worlds/:world_id/favorites`     | `entities/World/routes/updateWorldFavorites.ts`            | FED    |
| PATCH  | `/api/worlds/:world_id/likes`         | `entities/World/routes/updateWorldLikes.ts`                | FED    |
| PUT    | `/api/worlds/:world_id/highlight`     | `entities/World/routes/updateWorldHighlight.ts`            | FED    |
| PUT    | `/api/worlds/:world_id/ranking`       | `entities/World/routes/updateWorldRanking.ts`              | FED    |
| PUT    | `/api/worlds/:world_id/rating`        | `entities/World/routes/updateWorldRating.ts`               | FED    |
| PUT    | `/api/worlds/:world_id/featured`      | `entities/World/routes/featured.ts` (`featureWorld`)       | FED    |
| DELETE | `/api/worlds/:world_id/featured`      | `entities/World/routes/featured.ts` (`unfeatureWorld`)     | FED    |
| POST   | `/api/report`                         | `entities/Report/routes.ts`                                | FED    |
| PUT    | `/api/report/upload/:filename`        | `entities/Report/routes.ts` (upload)                       | FED    |
| GET    | `/api/map`                            | `entities/Map/routes/getMapPlaces.ts`                      | STUB   |
| GET    | `/api/map/places`                     | `entities/Map/routes/getAllPlacesList.ts`                  | STUB   |
| GET    | `/api/destinations`                   | `entities/Destination/routes/getDestinationsList.ts`       | STUB   |
| POST   | `/api/destinations`                   | `entities/Destination/routes/getDestinationsListById.ts`   | STUB   |

The upstream Express bootstrap also mounts `status()` and a catch-all 404
under `/api`. We replicate `/api/status` as a liveness probe and let axum's
default 404 cover the catch-all.

## Mounted under `/places/`

| Method | Path                  | Upstream file                                   | Status |
|--------|-----------------------|-------------------------------------------------|--------|
| GET    | `/places/place/`      | `entities/Social/routes.ts` (`injectPlaceMetadata`) | STUB |
| GET    | `/places/world/`      | `entities/Social/routes.ts` (`injectWorldMetadata`) | STUB |

These return HTML (OG metadata for share-link previews) — we serve the
inline `SOCIAL_HTML_TEMPLATE` from upstream with the share-default fields
filled in. Per-place data injection lands when `worlds` + per-place title
lookups are in scope.

## Local additions — admin (not in upstream)

Gated on `require_admin_bearer` (`handlers/admin.rs`); no upstream equivalent.

| Method | Path                             | Status |
|--------|-----------------------------------|--------|
| GET    | `/api/reports`                   | ADMIN  |
| PATCH  | `/api/reports/:id`                | ADMIN  |
| PATCH  | `/api/places/:place_id/disable`   | ADMIN  |
| GET    | `/api/pois`                      | ADMIN  |
| POST   | `/api/pois`                      | ADMIN  |
| PATCH  | `/api/pois/:position`             | ADMIN  |
| DELETE | `/api/pois/:position`             | ADMIN  |

## Federation sync (not in upstream)

| Method | Path                           | Status |
|--------|----------------------------------|--------|
| GET    | `/federation/places/snapshot`  | GET    |
| GET    | `/federation/places/changes`   | GET    |

## Local additions — misc (not in upstream)

| Method | Path     | Notes                                                                     |
|--------|----------|---------------------------------------------------------------------------|
| GET    | `/ping`  | catalyrst convention; matches catalyrst-market `/ping` for smoke testing  |

## Totals

- **31 routes** registered upstream (29 under `/api`, 2 under `/places`)
- **4 GETs** ported against the archive (`places_events.place`)
- **9 STUBs** (read endpoints that depend on absent tables; empty-shape responses)
- **16 writes** implemented as `FED` (verify `Signed<T>` federation envelope, persist, gossip — see `handlers/federation.rs`)
- **7 local admin routes** (`ADMIN`, `handlers/admin.rs`)
- **2 federation sync routes** (`handlers/fed_sync.rs`)
- **1 local addition** (`/ping`)

> _Re-verified against code 2026-07-03 (docs-stale-audit); corrections applied._
