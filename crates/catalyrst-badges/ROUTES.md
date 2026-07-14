# catalyrst-badges routes

Rust port of `badges.decentraland.org` (key=`badges`). Read-only profile badge state over a dedicated
`badges` postgres DB on a shared PostgreSQL cluster. Port: the deployment's assigned port (`5145`; see
the deployment's `catalyrst-badges` env file). Success envelope: bare `{"data": ...}` (NOT `{ok, data}`).
All routes are `auth:none`. `{address}` is lowercased before lookup.

| Method | Path | Status | Response shape | Unity client |
|---|---|---|---|---|
| GET | `/categories` | implemented | `{"data":{"categories":["string",...]}}` | `FetchBadgeCategoriesAsync` |
| GET | `/users/{address}/preview` | implemented | `{"data":{"latestAchievedBadges":[{id,name,tierName,image}]}}` | `FetchLatestAchievedBadgesAsync` |
| GET | `/users/{address}/badges?includeNotAchieved={true\|false}` | implemented | `{"data":{"achieved":[BadgeData],"notAchieved":[BadgeData]}}` | `FetchBadgesAsync` |
| GET | `/badges/{badgeId}/tiers` | implemented | `{"data":{"tiers":[{tierId,tierName,description,assets,criteria{steps}}]}}` | `FetchTiersAsync` |
| POST | `/users/{address}/badges/{badgeId}` | implemented | `{"data":{"granted":true,address,badgeId,tierId}}` | admin (bearer) |
| DELETE | `/users/{address}/badges/{badgeId}` | implemented | `{"data":{"revoked":true,address,badgeId}}` | admin (bearer) |
| GET | `/ping` | implemented | `pong` (health) | - |

## Admin grant/revoke (bearer-gated)

`POST`/`DELETE /users/{address}/badges/{badgeId}` are the only mutating routes, gated by
`Authorization: Bearer <CATALYRST_BADGES_ADMIN_TOKEN>`, compared in constant time; unset env fails
closed (403). Read-only routes are unaffected. Both are idempotent and 404 on unknown badge id.
Grant (`POST`): non-tier badge -> mark complete (`completed_at=now`, `steps_done>=1`); tier badge ->
record an achieved tier, optional body `{"tierId":"<id>"}` selects it, default is the highest-ordinal
tier. Revoke (`DELETE`): deletes the user's progress + achieved-tier rows for the badge.

`BadgeData` = `{id,name,description,category,isTier,completedAt,assets{2d{normal,hrm,baseColor},3d{...}},progress{stepsDone,nextStepsTarget,totalStepsTarget,lastCompletedTierAt,lastCompletedTierName,lastCompletedTierImage,achievedTiers[{tierId,completedAt}]}}`
- matches Unity `DCL.BadgesAPIService.BadgeData` exactly (assets keys `2d`/`3d`).

All timestamp fields (`completedAt`, `lastCompletedTierAt`, `achievedTiers[].completedAt`) are emitted
as Unix-epoch-millisecond strings (e.g. `"1749470400000"`): the Unity client parses them with
`long.Parse(...)` + `FromUnixTimeMilliseconds` (`BadgesUtils.FormatTimestampDate`); RFC3339/ISO strings
would throw a `FormatException` in the passport date renderer.

## Schema (migrations/)

- `0001_initial.sql` - `badge_definitions`, `badge_tiers`, `user_badge_progress`, `user_achieved_tiers`. Categories derived via `DISTINCT category`.
- `0002_seed_fixture.sql` - Stage-1 static fixture for definitions + tiers so the Unity passport renders against a fresh DB (idempotent `ON CONFLICT DO NOTHING`).
- `0003_admin_grant_audit.sql` - nullable `granted_by` (both user tables) and `granted_at` (achieved-tiers) recording provenance of admin grants.

Staging: Stage 1 (done) = read API over the DB, definitions/tiers seeded from fixture, per-user
progress tables populated out-of-band. Stage 2 (deferred until an event source is chosen) = event
consumer advancing `user_badge_progress` / `user_achieved_tiers`; the read routes already source
progress live from those tables once written.

Caching: in-process `moka` TTL caches (300s) for the derived category list and per-badge tier
definitions. Per-user reads are uncached (progress changes frequently). No Redis/S3/SQS - disk +
postgres + in-process cache only.
