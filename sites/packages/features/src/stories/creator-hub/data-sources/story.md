---
id: creator-hub-data-sources
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Publishing one ledger of every datum the creator hub can show -- its
    endpoint, its class, and whether it answered on this very request -- makes
    the hub's omissions legible, so a creator stops reading a missing number as
    a broken page and a contributor stops rediscovering endpoints that were
    excluded on purpose.
  because: >-
    The expensive failure in this product is not a missing figure, it is an
    invented one. A ledger that probes its own live rows is the cheapest
    possible check on every other screen: any screen that claims more than this
    page can prove is wrong, and the disagreement is visible in one click.
metric:
  primary: ch_data_sources_viewed_rate
  guardrails: []
decision:
  rule: >-
    Ship if ch_data_sources_viewed_rate clears the MDE with no regression in the
    activity surfaces' own view volume; otherwise keep the ledger as the
    per-screen footer only.
experiment:
  key: ch_data_sources
  unit: session
  baseline: 0.05
  mde: 0.015
  min_sample: 5000
  variants:
    - id: default
      weight: 1
      flags: {}
---

# Story -- creator-hub-data-sources

Single-surface screen at `/creator-hub/data-sources`. One variant; the
frontmatter is there for `storyLoader` and the id gate, **not** for a test.

## What it renders

Every datum the hub can show, grouped by class, with a filter strip. Columns:
Datum - Endpoint - State (probed) - Used by.

## Data reality

**`live` and `sampled` rows are probed on this request**, in parallel, with a 4
second ceiling, so the ledger cannot claim "live" for something that is down.
Those rows render a real `DatumBadge` plus "probed just now".

**`unbuilt` and `excluded` rows are constants and are never probed** -- probing
something that does not exist is theatre. They render a static class chip. The
invariant *probe is defined if and only if the class is `live` or `sampled`* is
asserted in `data-sources.test.ts`, not left to review.

Groups, in order:

| Group | Rows |
|---|---|
| `* LIVE` | your worlds (wcs `/worlds?authorized_deployer=`) - deployed bytes + quota (wcs `/wallet/{addr}/stats`) - users online per world (wcs `/live-data`) - platform totals (wcs `/status`) - likes/favourites (places `/api/worlds?names=`) - scene URN + spawn (catalyst `/world/{n}/about`) - ACLs + owner (catalyst `/world/{n}/permissions`) - realm health (catalyst `/about`) - your NAMEs (catalyst `/lambdas/users/{a}/names`) |
| ` SAMPLED` | headcount per world - headcount per scene - occupancy history per world - occupancy history per scene -- all `catalyst /presence/*`, cadence 300 s. Group note: **"A missing bucket is not a zero. Rows exist only for the instants a world or scene was live when the sampler ran. History depth is whatever exists, not a fixed window."** |
| ` SNAPSHOT` | *(none)* -- "The vocabulary exists and nothing currently qualifies." |
| ` UNAVAILABLE` | sessions/retention/device/FPS (creators-data `/creators/me/scenes/stats` -- 404) - the 16 world metrics (creators-data `/api/worlds/{w}/metrics` -- host serves the marketing SPA; the artifact reports `source: "fixture"`) - scene KV storage (`/world-storage/usage/*` -- 400 Invalid Auth Chain) - creator identity/studio/tier (creators-data `/api/me` -- not deployed) |
| ` NOT BUILT` | world federation / submit-to-another-realm - land & parcel event notifications - creator-defined metrics - crash reports scoped to my scene - in-scene bot spawn/control - live 2-D scene state - which Genesis parcels are mine -- each with the verified reason and a "Today:" line |
| ` EXCLUDED ON PURPOSE` | `comms.usersCount` / `bff.userCount` (always 0 on both realms) - `/hot-scenes` (fails open to `[]`, so `[]` is unreadable) - places `user_visits` / `user_count` (0 for every world sampled) - `/v2/parcels/{x}/{y}` (stub) - `occupancyTotals()` (`Math.max`es three disagreeing sources) - `worldsBase()` (resolves to `worlds.example.com`, which 404s every path). Group note: **"These are never rendered anywhere in the hub. This list exists so nobody rediscovers them and wires them up."** |

## The one rule that is not obvious

A row's **class** is what it ought to be; its **badge** is what happened on this
request. A `live`-class row whose probe failed shows an `Unavailable` badge
inside the `* LIVE` group. That is deliberate -- collapsing the two would either
hide today's outage or permanently demote a healthy endpoint.

## Status codes

Always `200`, including when every probe fails. On the activity screens "all
upstreams down" means the page has nothing to say and `503` is honest; here,
"everything is down" **is** the page's content, and swapping it for an
`UpstreamUnavailable` screen would hide the one report a reader came for.

## Authorization

None, and the page says so. Every endpoint listed answers unauthenticated
requests.

## Writes

None. `docs/simulated-write-paths.md` gains zero entries.
