---
id: creator-hub-activity
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving creators one surface that shows who is in their worlds right now --
    with the provenance of every figure attached, and the things we cannot
    measure named rather than omitted -- makes the hub's numbers trustworthy
    enough to act on, instead of numbers a creator has learned to discount.
  because: >-
    The hub's existing surfaces mix live reads, gated endpoints and absent
    backends into one undifferentiated wall of figures, so a creator cannot tell
    a real zero from a failed fetch. Attaching state to every value -- and saying
    plainly which questions this stack cannot answer -- is what makes the ones it
    can answer worth reading.
metric:
  primary: ch_activity_world_opened_rate
  guardrails: []
decision:
  rule: >-
    Ship if ch_activity_world_opened_rate (world detail opened per activity
    view) clears the MDE with no drop in ch_activity_viewed; otherwise hold and
    revisit which figures earn the top of the page.
experiment:
  key: ch_activity
  unit: session
  baseline: 0.12
  mde: 0.03
  min_sample: 2400
  variants:
    - id: default
      weight: 1
      flags: {}
---

# Story -- creator-hub-activity

Single-surface screen at `/creator-hub/activity`. There is one variant. The
frontmatter exists because `storyLoader` parses it from disk at request time and
`gen:story-ids:check` gates the id -- **this surface is not being A/B tested**.

## What it renders

Your worlds with a live headcount, realm context, and a Genesis-parcel lookup
via `?pointer=x,y`. Headcount only.

## Data reality

Every figure on this screen is a `Datum` -- a value *and* its provenance. The
non-showable states have no `value` field at all, so there is no way to reach for
a fallback number.

| Datum | Source | Live | Stale | Unavailable | Unbuilt |
|---|---|---|---|---|---|
| People in your worlds right now | wcs `GET /live-data`, summed over `perWorld[]` for your worlds | `5` + `* LIVE` | n/a -- real-time read, staleness undefined | `--` + ` UNAVAILABLE`, `GET worlds-content-server.decentraland.org/live-data returned {status}` | n/a |
| Network peers / islands | catalyst `GET /presence/current` -> `peers_count`, `islands_count` | `22 peers, 8 islands` + ` SAMPLED - 2m ago` | `> 15 min` -> ` STALE` + "the sampler may have stopped" | `--` + `` | n/a |
| World rows: name, title, `last_deployed_at`, `deployed_scenes`, `blocked_since` | wcs `GET /worlds?authorized_deployer={addr}&limit=100&sort=last_deployed_at&order=desc` | table | n/a | **whole table replaced by one unavailable panel**, never an empty table | n/a |
| Row -> Now | `GET /presence/current/worlds`, matched on `world_name` | `* 2` | ` STALE` on the section header | per-row `--` + footnote | n/a |
| Row -> Peak 7d | `GET /presence/worlds/history?world={n}&limit=2016` | `6` | as above | per-row `--` | n/a |
| Busiest scenes / worlds now | `GET /presence/current/scenes`, `/current/worlds` | list with real `scene_name` | ` STALE` | section -> `` panel | n/a |
| Your Genesis parcels | -- | n/a | n/a | n/a | ` NOT BUILT` -- nothing on this stack maps a wallet to the parcels it deployed to. **Today:** a working `x,y` lookup that sets `?pointer=` and renders the same history from `/presence/scenes/history`. |
| Who they were, how long they stayed, device | -- | n/a | n/a | n/a | ` NOT BUILT` -- presence persists addresses but its HTTP API returns counts only; `/creators/me/scenes/stats` 404s. **Today:** the headcounts above are the whole picture. |

### The per-row join rule

This table is the acceptance criteria. A world absent from the presence snapshot
and a world sampled at zero are **different facts** and the codebase must never
be able to confuse them.

| Condition | Renders |
|---|---|
| `deployed_scenes > 0`, in presence sample, `count > 0` | `* 2` |
| `deployed_scenes > 0`, in presence sample, `count === 0` | `0` + **mandatory** note "a real zero -- sampled at {t}, nobody in" |
| `deployed_scenes > 0`, **absent** from presence sample | `--` + "No sample: this world was not live at the last snapshot. Not the same as zero." |
| `deployed_scenes === 0` | ` NEVER DEPLOYED` + "No scene deployed to this NAME, so presence has never had anything to sample." + `[ Publish a scene here ]` |
| `blocked_since` set | ` BLOCKED` + the date |

### Sums, and what is not summed

"People in your worlds right now" is the sum of `/live-data`'s `perWorld[]`
entries across your worlds -- deliberately **not** `data.totalUsers`, which is
every world on the platform. If either the world list or `/live-data` failed, the
tile shows the datum that failed, not a partial sum.

`occupancyTotals()` is never used for a per-world display: it `Math.max`es three
disagreeing sources into one figure.

## Authorization

None, and the page says so. No route in this app rejects an unauthenticated
request; wcs, `/presence/*` and the Places API all answer anonymously. The
address selects which rows you see; it does not protect them. The no-address
state offers a lookup field precisely to make that legible -- it is scoping, not
a sign-in wall. There is no `DEMO_OWNER` fallback.

## Writes

None. This screen reads. `docs/simulated-write-paths.md` gains zero entries.

## Status codes

`200` partially degraded - `503` every upstream down - the page never renders a
figure it did not read.
