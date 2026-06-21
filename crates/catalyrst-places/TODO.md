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

## 501 write endpoints — federation wiring needed

Every `// TODO(federation): ...` comment in
`src/handlers/federation_stubs.rs` marks a handler that:

1. Today returns `501 Not Implemented` with body
   `{"ok": false, "message": "...", "federation_adr": "<the federation specification>"}`.
2. Needs to be replaced with: parse the body, build a `Signed<T>`
   envelope via `catalyrst_fed::Signed`, verify the signature against the
   authenticated session per ADR §2.3, append to `signed_actions_places`
   in a single transaction with the materialised-table trigger, then
   gossip via NATS subject `federation.places.opinions` per ADR §4.

Mapping of route → signed-action type (already defined in `src/fed_actions.rs`):

| Route | Signed action type |
|---|---|
| `PATCH /api/places/:id/favorites`        | `PlaceFavorite { action: Add\|Remove }` |
| `PATCH /api/places/:id/likes`            | `PlaceVote { score: +1\|0\|-1 }` |
| `PUT   /api/places/:id/rating`           | `PlaceVote` (same wire type; `rating` is a UX synonym for vote) |
| `POST  /api/report`                      | `PlaceReport { reason: enum }` |
| `PATCH /api/worlds/:id/favorites`        | `PlaceFavorite` (world `place_id`) |
| `PATCH /api/worlds/:id/likes`            | `PlaceVote` (world `place_id`) |
| `PUT   /api/worlds/:id/rating`           | `PlaceVote` |
| `PUT   /api/places/:id/{highlight,ranking,featured}`<br>`PUT /api/worlds/:id/{highlight,ranking,featured}`<br>`DELETE /api/{places,worlds}/:id/featured` | Curator action — needs a separate ADR; not in the §3 schemas. Likely a `PlaceCuratorAction` signed by an admin-scoped session key per ADR §2.3. |

## Federation transport / state

- **Storage:** `signed_actions_places` table + `place_favorites_current` /
  `place_vote_score_current` materialised tables + insert-time trigger per
  ADR §2. Not yet created — needs a migration owned by the federation
  agent.
- **Snapshot endpoint:** `GET /federation/v1/places/snapshot?since=<unix_ts>`
  per ADR §4. Not yet exposed.
- **NATS subscription:** `federation.places.opinions` per ADR §4. Not yet
  wired.
- **Rate limits:** ADR §5 table — `catalyrst_fed::RateLimiter` primitive
  exists; needs binding to the write handlers.
- **Namespace resolution:** every signed action's `place_id` must resolve to
  an entity in the local content-server store (genesis-city scene) or
  worlds-content-server (worlds entity). Lookup helper needs to live in
  `catalyrst-server` so it's shared with `catalyrst-events`.

## Local additions vs upstream

- `/ping` — catalyrst convention for smoke testing; mirrors
  `catalyrst-market`. Not in upstream.
- The 501 body carries `federation_adr` link — not in upstream (upstream
  has no 501 paths).
