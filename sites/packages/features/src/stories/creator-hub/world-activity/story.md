---
id: creator-hub-world-activity
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Answering several jobs about one published world on a single scrolling
    surface -- right-now occupancy, occupancy history, what is deployed, who can
    get in, and how it was received -- with the four things we cannot answer
    sitting adjacent to the ones we can, gives a creator a reason to return
    between deploys.
  because: >-
    A creator's questions about a world are not separable: "is anyone in it" and
    "did my deploy land" are the same visit. Filing the gaps behind a tab would
    hide exactly the information that makes the rest credible, so they stay on
    the page, dashed and unclickable.
metric:
  primary: ch_world_activity_returned_rate
  guardrails: []
decision:
  rule: >-
    Ship if ch_world_activity_returned_rate (sessions with a second visit to a
    world page within 7 days) clears the MDE with no regression in
    ch_world_activity_viewed; otherwise hold and revisit the section order.
experiment:
  key: ch_world_activity
  unit: session
  baseline: 0.09
  mde: 0.025
  min_sample: 3000
  variants:
    - id: default
      weight: 1
      flags: {}
---

# Story -- creator-hub-world-activity

Single-surface screen at `/creator-hub/activity/:world`. One variant; the
frontmatter is there for `storyLoader` and the id gate, **not** for a test.

Single scrolling page, no tabs. Sections in order: Right now - Who was here -
What is deployed - Who can get in - Reception - Not built yet - Source ledger.

## Data reality

| Section | Datum | Source | Live | Stale | Unavailable | Unbuilt |
|---|---|---|---|---|---|---|
| Header | title, owner, `last_deployed_at`, `deployed_scenes`, `blocked_since` | wcs `/worlds?authorized_deployer=` row, else catalyst `GET /world/{n}/about` | rendered | n/a | header degrades to the bare world name + `` note | n/a |
| Right now | In this world | `GET /presence/current/worlds` -> `count` | `2` + ` SAMPLED - 2m` | `>15m` -> ` STALE` | `--` + `` | n/a |
| Right now | Comms room | wcs `/live-data` -> `perWorld[].users` | `3` + `* LIVE` | n/a | `--` + `` | n/a |
| Right now | Realm | catalyst `GET /about` -> `healthy`, `acceptingUsers`, `synchronizationStatus` | "healthy, accepting users" + `* LIVE` | n/a | `--` + `` | n/a |
| Right now | *disagreement line* | derived | **Mandatory whenever the two differ:** "These disagree (2 vs 3) and both are right: presence samples every 5 minutes and counts distinct addresses in comms; `/live-data` is instant and is the worlds server's own figure. Neither is 'users online'." | -- | omitted when either side is unavailable | -- |
| History | occupancy series | `GET /presence/worlds/history?world={n}&limit=5000` | `AnalyticsChart`, nulls break the path, `gapBands` hatched | section header ` STALE` | chart region -> error empty-state naming the endpoint + ``; **the Right-now tiles above are unaffected** | n/a |
| History | Peak concurrent (sampled) / Snapshots with someone in it / History begins | derived | rendered with those exact labels | -- | `--` | -- |
| Deployed | scene URN, spawn coords | catalyst `GET /world/{n}/about` -> `configurations.scenesUrn[0]`, `spawnCoordinates` | rendered + `* LIVE` | n/a | `--` + `` | n/a |
| Deployed | bytes for this NAME / wallet quota | wcs `GET /wallet/{addr}/stats` -> `dclNames[].size`, `usedSpace`, `maxAllowedSpace` | `59.8 MB of 6.6 GB` + bar + `* LIVE` | n/a | `--` + `` | n/a |
| Deployed | Scene key-value storage | catalyst `GET /world-storage/usage/{world\|players\|env}` | **unreachable** | -- | **always ` UNAVAILABLE`**: 400 Invalid Auth Chain -- it needs an ADR-44 signed fetch made by the *scene runtime*; this hub holds no such identity. It would also be a different number. **Never a `0 B` tile.** | -- |
| Access | `owner`, `deployment.{type,wallets}`, `streaming`, `access.type` | catalyst `GET /world/{n}/permissions` | rendered, **read-only** | n/a | `` panel (an unread ACL and an empty ACL are not the same thing) | n/a |
| Access | changing it | -- | -- | -- | -- | `CliEscape` for `dcl-one-sdk world permissions grant ...`, plus a plain link to `/creator-hub/world-permissions`, which owns that flow |
| Reception | likes, dislikes, favourites, `like_rate`, `deployed_at` | `GET places.decentraland.org/api/worlds?names={n}` | rendered + `* LIVE` | n/a | `--` per field + `` | n/a |
| Reception | *exclusion sentence* | -- | **Mandatory:** "That response also carries `user_visits` and `user_count`. Both read 0 for every world we sampled, so they are not shown." | -- | -- | -- |
| Not built | Sessions & retention | -- | -- | -- | -- | `` -- `/creators/me/scenes/stats` 404s; the client half exists, the route does not. **Today:** the headcount above. |
| Not built | Did it break? | -- | -- | -- | -- | `` -- telemetry reads are admin-gated and carry no scene or owner dimension. **Today:** a copyable `get_scene_logs` MCP call; the browser cannot reach an explorer's `--mcp` port, so there is **no Run button**. |
| Not built | Tell me when it changes | -- | -- | -- | -- | `` -- notifications is email preferences with zero parcel/land/scene references. **Today:** the `` on this page. |
| Not built | Live 2-D scene state | -- | -- | -- | -- | `` -- nothing serves a scene's current entity or player state. **Today:** join the world. |

## Full-page states

| Condition | Response |
|---|---|
| world not in the caller's wcs list **but** `/world/{n}/about` 200 | **200, render the page**, plus a neutral line: occupancy is public, anyone can read these counts for any world. **No lock, no "you must own this".** |
| world absent from wcs **and** `/world/{n}/about` 404 | `404` + error empty-state naming both hosts + `[ See your worlds ]` |
| every upstream failed | `503` + `UpstreamUnavailable` naming the hosts |
| presence down, wcs up | **200.** Right-now and History become unavailable panels; Deployed / Access / Reception stay live. Partial degradation must be partial. |
| history returns `[]`, world is deployed | inline empty-state: the collector has no snapshots for this world -- it has not been in the poll set. **Not a zero series.** |
| rows exist, none in the last 15 min | `0` + "a real zero -- nobody in it at the last snapshot" |

## Authorization

None. Ownership is context, never a gate. The address scopes rows; it does not
protect them. Every source on this page answers unauthenticated requests.

## Writes

None. Permissions render read-only and the write path is a copyable command plus
a link to the route that owns it. `docs/simulated-write-paths.md` gains zero
entries.
