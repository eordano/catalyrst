# catalyrst-places — TODO

Tracks what's stubbed in this port and what the federation/write phase needs
to wire. Mirrors `ROUTES.md`'s status column.

## Schema divergence — archive `places_events.place` vs upstream `places`

The upstream `places-server` schema (TypeORM migrations under
`decentraland/places/src/migrations/`) has columns the daily archive snapshot
does NOT carry. Documented here because every `GET` handler in this crate
silently treats the missing data as the empty / falsy default.

### Top-3 blockers for full GET parity

1. **`textsearch` ts_vector column is absent.** Upstream `/api/places?search=foo`
   uses `to_tsquery(...)` against `place.textsearch` with `ts_rank_cd`-ordered
   results. The archive doesn't materialise that column. This crate falls back
   to `title ILIKE %s%` + `description ILIKE %s%`, which returns the same
   *set* of rows for most queries but in a different *order*. Either (a)
   re-materialise `textsearch` inside the catalyst's own places store after
   federation lands, or (b) accept the order-only drift as a documented
   divergence in `places-parity.sh`.

2. **No `place_positions` / `worlds` / `place_categories` / `destinations`
   tables.** Eight upstream routes (`/api/map`, `/api/map/places`,
   `/api/destinations`, `/api/worlds*`, `/api/categories`, `/api/places/:id/categories`)
   read these tables exclusively. The archive's `place.raw` JSONB blob carries
   the *upstream API response*, which includes `positions` and `categories`
   per-place, but the relational tables that let the live server filter by
   parcel position or category-with-count don't exist. All these routes are
   marked STUB and return the upstream empty-shape envelope. Wiring full
   support requires either: (a) federation lands first and gives the catalyst
   its own write store, or (b) the archive grows the missing tables (would
   require a places-archive.py extension).

3. **No per-user state — `user_favorite`, `user_like`, `user_dislike`,
   `user_count`, `user_visits` are always `false`/`0`.** Per-user data
   (favorites, likes, scene-stats user visits) is precisely what the
   federation signed-action log will carry — see `docs/federation/places.md`
   §2. The Rust port computes `like_rate = likes / (likes + dislikes)` and
   sets `like_score = like_rate` as a temporary stand-in for the upstream's
   rank-decay formula. Once `signed_actions_places` + the materialised
   `place_favorites_current` / `place_vote_score_current` tables exist, the
   `find_*` queries in `ports/places.rs` need to JOIN them in.

## Stubbed read endpoints

For each STUB in `ROUTES.md`, this section pins what the handler would need:

- `GET /api/categories` — needs a categories source (upstream reads from
  `intl/en.json` + an aggregate count over `place_categories`). Returns
  `{ok: true, data: []}`. **Fix path:** ship a baked-in category list from
  `decentraland/places/src/intl/en.json` and compute counts via
  `unnest(categories)` on the archive's `place.categories`.
- `GET /api/places/:place_id/categories` — needs `place_categories` table.
  Returns `{ok: true, data: {categories: []}}`. **Fix path:** read
  `categories` column directly from `place.categories` text array.
- `GET /api/worlds`, `GET /api/worlds/:world_id`, `GET /api/world_names`,
  `PATCH /api/worlds/:world_id/{favorites,likes}`, `PUT /api/worlds/:world_id/{highlight,ranking,rating,featured}`,
  `DELETE /api/worlds/:world_id/featured` — needs a `worlds` table. **Fix
  path:** the archive's `place.raw->>'world' = true` rows ARE world entries;
  re-materialise the upstream worlds-table view on top of them.
- `GET /api/map`, `GET /api/map/places` — needs `place_positions` for
  parcel→place lookup and hot-scenes overlay. **Fix path:** unnest
  `place.positions` into a CTE; hot-scenes overlay needs realm-provider
  integration (out of scope for the archive-only port).
- `GET /api/destinations`, `POST /api/destinations` — depends on `worlds`
  + `place_positions` + the destinations join. **Fix path:** blocked on
  the same items as `/api/worlds` and `/api/map`.
- `GET /places/place/`, `GET /places/world/` — serves OG metadata HTML for
  share-link previews. Stub returns the inline `SOCIAL_HTML_TEMPLATE` with
  empty defaults; per-place title/description injection needs the
  `replaceHelmetMetadata` analogue (not in scope).

## Federation write endpoints — DONE (2026-07-03)

**This section previously described these as `501 Not Implemented` stubs in
`src/handlers/federation_stubs.rs`. That file never existed / is gone; the
real, implemented module is `src/handlers/federation.rs` (617 lines). All of
the writes below are live — no route in this crate returns 501.**

`favorites` / `likes` / `report` accept either a direct authenticated request
(`auth_address_verified`) or a federation envelope (`is_federation_envelope`
dispatches to `handlers/federation.rs`'s `preflight::<T>`, which parses the
`Signed<T>` body via `catalyrst_fed::Signed`, verifies the signature, and
replay-checks it via `fed::replay::check_and_record` before persisting
through `fed::apply` and gossiping via `state.gossip`). `rating` / `ranking`
/ `highlight` / `featured` are curator actions gated on admin / data-team
bearer tokens (`require_admin`, `require_bearer_token`) rather than the
`Signed<T>` envelope — they are still fully implemented, just not
peer-signed writes.

Mapping of route → signed-action type (`src/fed/messages.rs`):

| Route | Signed action type |
|---|---|
| `PATCH /api/places/:id/favorites`        | `PlaceFavorite { action: Add\|Remove }` |
| `PATCH /api/places/:id/likes`            | `PlaceVote { score: +1\|0\|-1 }` |
| `PUT   /api/places/:id/rating`           | curator action (admin-bearer gated, not `Signed<T>`) |
| `POST  /api/report`                      | `PlaceReport { reason: enum }` |
| `PATCH /api/worlds/:id/favorites`        | `PlaceFavorite` (world `place_id`) |
| `PATCH /api/worlds/:id/likes`            | `PlaceVote` (world `place_id`) |
| `PUT   /api/worlds/:id/rating`           | curator action (admin-bearer gated, not `Signed<T>`) |
| `PUT /api/places/:id/{highlight,ranking,featured}`<br>`PUT /api/worlds/:id/{highlight,ranking,featured}`<br>`DELETE /api/{places,worlds}/:id/featured` | Curator action — `require_admin` / `require_bearer_token(data_team_auth_token / admin_auth_token)`, direct DB write via `state.places.set_*`. No separate `PlaceCuratorAction` signed type; these are not gossiped. |

## Federation transport / state

- **Storage:** `signed_actions_places`, `user_favorites`, `user_likes`,
  `place_reports_local`, `pois`, `seen_nonces` — all created inline via
  `CREATE TABLE IF NOT EXISTS` in `src/ports/places.rs` (no separate
  migration file for this crate). **DONE (2026-07-03).**
- **Snapshot endpoint:** `GET /federation/places/snapshot` (note: no `/v1`
  segment — differs from the originally-planned path) is registered in
  `src/lib.rs` and implemented in `src/handlers/fed_sync.rs::snapshot`.
  **DONE (2026-07-03).**
- **Changes endpoint:** `GET /federation/places/changes?since=<seq>` —
  implemented alongside the snapshot endpoint in `fed_sync.rs::changes`.
  **DONE (2026-07-03).**
- **Gossip subscription:** wired via `catalyrst_fed::GossipPublisher`
  (`Scope::Places`), consumed in `src/fed/consumer.rs::spawn`/`run`, which
  verifies + replay-checks + applies incoming peer envelopes. **DONE
  (2026-07-03).** (Transport-level subject naming is internal to
  `catalyrst_fed::GossipPublisher`, not a literal `federation.places.opinions`
  NATS subject as originally planned.)
- **Rate limits:** still open — no `RateLimiter` usage found anywhere in
  this crate (`grep RateLimiter` returns nothing). Needs binding to the
  write handlers.
- **Namespace resolution:** still open — no shared lookup helper in
  `catalyrst-server` was found; each crate resolves `place_id`/`entity_id`
  independently today.

## Local additions vs upstream

- `/ping` — catalyrst convention for smoke testing; mirrors
  `catalyrst-market`. Not in upstream.
- `/api/reports`, `/api/reports/:id`, `/api/places/:place_id/disable`,
  `/api/pois`, `/api/pois/:position` — admin-only routes (`handlers/admin.rs`),
  not in upstream.
- `/federation/places/snapshot`, `/federation/places/changes` — federation
  sync routes (`handlers/fed_sync.rs`), not in upstream.

> _Re-verified against code 2026-07-03 (docs-stale-audit); corrections applied._
