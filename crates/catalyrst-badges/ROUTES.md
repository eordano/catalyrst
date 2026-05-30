# catalyrst-badges routes

Rust port of `badges.decentraland.org` (key=`badges`). Read-only profile badge
state over a dedicated `badges` postgres DB on a shared PostgreSQL cluster.

Port: the deployment's assigned port (`5141`). Success envelope: bare `{"data": ...}` (NOT `{ok, data}`).
All routes are `auth:none`. `{address}` is lowercased before lookup.

| Method | Path | Status | Response shape | Unity client |
|---|---|---|---|---|
| GET | `/categories` | implemented | `{"data":{"categories":["string",...]}}` | `FetchBadgeCategoriesAsync` |
| GET | `/users/{address}/preview` | implemented | `{"data":{"latestAchievedBadges":[{id,name,tierName,image}]}}` | `FetchLatestAchievedBadgesAsync` |
| GET | `/users/{address}/badges?includeNotAchieved={true\|false}` | implemented | `{"data":{"achieved":[BadgeData],"notAchieved":[BadgeData]}}` | `FetchBadgesAsync` |
| GET | `/badges/{badgeId}/tiers` | implemented | `{"data":{"tiers":[{tierId,tierName,description,assets,criteria{steps}}]}}` | `FetchTiersAsync` |
| GET | `/ping` | implemented | `pong` (health) | — |

`BadgeData` = `{id,name,description,category,isTier,completedAt,assets{2d{normal,hrm,baseColor},3d{...}},progress{stepsDone,nextStepsTarget,totalStepsTarget,lastCompletedTierAt,lastCompletedTierName,lastCompletedTierImage,achievedTiers[{tierId,completedAt}]}}`
— matches Unity `DCL.BadgesAPIService.BadgeData` exactly (assets keys `2d`/`3d`).

All timestamp fields (`completedAt`, `lastCompletedTierAt`, `achievedTiers[].completedAt`)
are emitted as Unix-epoch-millisecond **strings** (e.g. `"1749470400000"`), because
the Unity client parses them with `long.Parse(...)` + `FromUnixTimeMilliseconds`
(`BadgesUtils.FormatTimestampDate`). RFC3339/ISO strings would throw a
`FormatException` in the passport date renderer.

## Schema (migrations/)

- `0001_initial.sql` — `badge_definitions`, `badge_tiers`, `user_badge_progress`,
  `user_achieved_tiers`. Categories derived via `DISTINCT category`.
- `0002_seed_fixture.sql` — Stage-1 static fixture for definitions + tiers so the
  Unity passport renders against a fresh DB (idempotent `ON CONFLICT DO NOTHING`).

## Staging

- **Stage 1 (done):** read API over the DB; definitions/tiers seeded from fixture;
  per-user progress tables treated as populated out-of-band.
- **Stage 2 (deferred):** event consumer to advance `user_badge_progress` /
  `user_achieved_tiers`. Deferred until an event source is chosen. The four read
  routes are complete and source progress live from those tables once written.

## Caching

In-process `moka` TTL caches (300s): derived category list and per-badge tier
definitions. Per-user reads are uncached (progress changes frequently). No
Redis/S3/SQS — disk + postgres + in-process cache only.
