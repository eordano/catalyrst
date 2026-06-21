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
- `501`  — write endpoint, returns 501 Not Implemented pointing at
           the federation specification (federation owns this)

## Mounted under `/api/`

| Method | Path                                  | Upstream file                                              | Status |
|--------|---------------------------------------|------------------------------------------------------------|--------|
| GET    | `/api/categories`                     | `entities/Category/routes.ts`                              | STUB   |
| PATCH  | `/api/places/:entity_id/favorites`    | `entities/UserFavorite/routes.ts`                          | 501    |
| PATCH  | `/api/places/:entity_id/likes`        | `entities/UserLikes/routes.ts`                             | 501    |
| GET    | `/api/places/:place_id`               | `entities/Place/routes/getPlace.ts`                        | GET    |
| GET    | `/api/places`                         | `entities/Place/routes/getPlaceList.ts`                    | GET    |
| POST   | `/api/places`                         | `entities/Place/routes/getPlaceListById.ts`                | GET    |
| PUT    | `/api/places/:place_id/rating`        | `entities/Place/routes/updateRating.ts`                    | 501    |
| PUT    | `/api/places/:place_id/ranking`       | `entities/Place/routes/updateRanking.ts`                   | 501    |
| PUT    | `/api/places/:place_id/highlight`     | `entities/Place/routes/updateHighlight.ts`                 | 501    |
| GET    | `/api/places/:place_id/categories`    | `entities/Place/routes/getPlaceCategories.ts`              | STUB   |
| PUT    | `/api/places/:place_id/featured`      | `entities/Place/routes/featured.ts` (`featurePlace`)       | 501    |
| DELETE | `/api/places/:place_id/featured`      | `entities/Place/routes/featured.ts` (`unfeaturePlace`)     | 501    |
| POST   | `/api/places/status`                  | `entities/Place/routes/getPlaceStatusListById.ts`          | GET    |
| GET    | `/api/worlds/:world_id`               | `entities/World/routes/getWorld.ts`                        | STUB   |
| GET    | `/api/worlds`                         | `entities/World/routes/getWorldList.ts`                    | STUB   |
| GET    | `/api/world_names`                    | `entities/World/routes/getWorldNamesList.ts`               | STUB   |
| PATCH  | `/api/worlds/:world_id/favorites`     | `entities/World/routes/updateWorldFavorites.ts`            | 501    |
| PATCH  | `/api/worlds/:world_id/likes`         | `entities/World/routes/updateWorldLikes.ts`                | 501    |
| PUT    | `/api/worlds/:world_id/highlight`     | `entities/World/routes/updateWorldHighlight.ts`            | 501    |
| PUT    | `/api/worlds/:world_id/ranking`       | `entities/World/routes/updateWorldRanking.ts`              | 501    |
| PUT    | `/api/worlds/:world_id/rating`        | `entities/World/routes/updateWorldRating.ts`               | 501    |
| PUT    | `/api/worlds/:world_id/featured`      | `entities/World/routes/featured.ts` (`featureWorld`)       | 501    |
| DELETE | `/api/worlds/:world_id/featured`      | `entities/World/routes/featured.ts` (`unfeatureWorld`)     | 501    |
| POST   | `/api/report`                         | `entities/Report/routes.ts`                                | 501    |
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

## Local additions (not in upstream)

| Method | Path     | Notes                                                                     |
|--------|----------|---------------------------------------------------------------------------|
| GET    | `/ping`  | catalyrst convention; matches catalyrst-market `/ping` for smoke testing  |

## Totals

- **31 routes** registered upstream (29 under `/api`, 2 under `/places`)
- **4 GETs** ported against the archive (`places_events.place`)
- **13 STUBs** (read endpoints that depend on absent tables; empty-shape responses)
- **14 writes** stubbed as `501 Not Implemented` (federation-owned per ADR)
- **1 local addition** (`/ping`)
