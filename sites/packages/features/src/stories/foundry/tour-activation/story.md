---
id: foundry-tour-activation
status: running
owner: redacted@example.com
hypothesis:
  statement: >-
    Auto-starting the guided tour on a visitor's first /foundry front-door view
    raises the share of exposed visitors who take a first meaningful action -
    measured as submitting a pledge on the Exchange - versus keeping the tour
    opt-in behind the header button.
  because: >-
    The Foundry front door is dense: seven rail cards over four surfaces (the
    games, the design docs, the copilot, the console). The tour's second step
    walks the visitor directly to the Exchange and asks them to pledge - the
    cheapest real, shared-state action on the site, and the only one a visitor
    can take without leaving for a game client. Opt-in tours are discovered by
    almost nobody; auto-opening moves every first-time visitor onto the guided
    path at the moment their intent is highest, at the risk of annoying
    visitors who wanted to explore freely (bounded by the dismissal guardrail).
metric:
  primary: fd_pledge_submitted_rate
  numerator: fd_pledge_submitted
  denominator: experiment_exposed
  guardrails:
    - fd_game_link_opened
    - fd_tour_dismissed
decision:
  rule: >-
    Read out once both arms reach min_sample exposures (story-readout
    two-proportion z-test, alpha 0.05 two-sided). Ship auto-tour if
    fd_pledge_submitted_rate beats control by at least the MDE (+0.04
    absolute) AND both guardrails hold: fd_game_link_opened per exposure does
    not drop more than 10% relative to control, and in the auto arm
    fd_tour_dismissed on steps 1-2 stays under half of fd_tour_started.
    Otherwise keep the opt-in tour. No interim look before min_sample.
experiment:
  key: foundry-tour-activation
  unit: session
  variants:
    - id: control
      weight: 1
      flags: {}
    - id: auto-tour
      weight: 1
      flags:
        tourAutoStart: true
  baseline: 0.08
  mde: 0.04
  min_sample: 1000
---

# Story — Foundry tour activation

`/foundry` is the front door of an open build bench: seven games that
Decentraland creators really deployed to Worlds between April and July 2026,
the shortGDDs that document what a game promises, a self-hosted copilot with
its token bill on the page, and a console of bot runs, replayable episodes and
costs. It is dense on purpose — everything on it traces to a deployment entity,
a git history, a bot run or a token count — and density is the risk. A visitor
who reads and leaves produces no shared-state record.

The guided tour is the counter-measure: seven steps that walk the visitor
across the bench and, at step 2, park them on the Exchange with a pledge button
in front of them. A pledge is the cheapest real action on the site — one click,
one row in `foundry.pledge`, immediately visible to every other visitor. This
experiment asks whether the tour is worth opening for the visitor rather than
waiting to be found.

Assignment is per session, bucketed deterministically by `sid`
(`assign.ts`, cyrb53 over `${sid}:foundry-tour-activation`).

## Arms

- **control** — the tour is opt-in. The Tour button sits in the ChromeShell
  `right` slot on every `/foundry` page; nothing opens on its own.
- **auto-tour** (`tourAutoStart: true`) — identical page. On the first
  front-door mount the route navigates to `?tour=1` and the tour opens at step 1
  with `fd_tour_started { source: "auto" }`. A `localStorage` latch makes this
  once per visitor, so a returning session sees the quiet page. The Tour button
  is present in this arm too, and a manual open still reports
  `source: "button"`.

The two arms render the same markup, the same copy, and the same entry points.
The only difference is whether the tour card is open on arrival.

Tour state lives in the URL (`?tour=<1..7>`), so every step is shareable and
survives reload; Esc or the End control clears the param. Preview either arm
with `?variant=foundry-tour-activation:auto-tour` (or `:control`).

## Events

| event | when | role |
| --- | --- | --- |
| `experiment_exposed` | the `/foundry` front-door loader resolves the assignment | exposure (denominator) |
| `fd_home_viewed` | front door mounts; carries the registered-game, design-doc and bench-run counts plus the copilot probe | diagnostic |
| `fd_tour_started` | the tour opens — `{ page, source: "auto" \| "button", steps }` | treatment reach |
| `fd_tour_step_viewed` | each step renders — `{ page, step, step_id, steps }` | step funnel |
| `fd_pledge_submitted` | the Exchange action commits a pledge row — `{ pledges_after, request_id }` | **primary numerator** |
| `fd_pledge_retracted` | a pledge is withdrawn — `{ pledges_after, request_id }` | counter-signal for toggle-cycling |
| `fd_game_link_opened` | a visitor leaves for a game — `{ slug, target: "play" \| "editor" \| "world" }` | guardrail (deep engagement holds) |
| `fd_tour_dismissed` | the tour is closed before the last step — `{ step, steps }` | guardrail (annoyance) |
| `fd_tour_completed` | the last step is reached — `{ steps }` | diagnostic |

`fd_pledge_submitted` and `fd_pledge_retracted` fire server-side in the route
action after the write commits, so the event count equals the row count. The
tour events and `fd_game_link_opened` fire client-side on mount and on click.

**Primary metric:** `fd_pledge_submitted_rate` = `fd_pledge_submitted` /
`experiment_exposed`, scoped to one variant.

## Wiring

The readout reads zero without all three of these:

1. Exposure fires **only** on the front door — `foundry._index.tsx` calls
   `storyLoader(request, "foundry/tour-activation", FALLBACK)`. Every other
   `/foundry` route resolves the same assignment with `{ skipExposure: true }`,
   so a visitor is exposed once, not once per page.
2. Every `track` call on every `/foundry` route passes
   `{ sid, story, variant, experimentKey }` — the readout joins arms on
   `properties->>'exp_key'`, and an untagged event is invisible to it.
3. `experimentKey: "foundry-tour-activation"` appears literally in the route
   FALLBACK, which is what ties the emitted `exp_key` to this frontmatter.

## Caveats

- **The readout counts event rows, not distinct sids.** This matches every
  existing story on this site. A visitor who pledges on three requests
  contributes three numerator rows against one exposure. `fd_pledge_retracted`
  is tracked as a counter-signal so pledge/withdraw cycling is visible in the
  readout rather than silently inflating the numerator.
- **`baseline: 0.08` is a stated estimate**, not a measurement — the surface is
  new and has no prior pledge rate. The readout compares the two arms against
  each other, so a wrong prior stretches the timeline to significance but
  cannot bias the verdict.
- **The board starts empty.** Requests and pledges are visitor writes and the
  Exchange ships with zero rows, so early exposures meet a board whose only
  content is what other visitors have posted. Until the first request exists the
  numerator can only be reached through the create-then-pledge path, which
  depresses both arms equally but slows the read.
- **`min_sample: 1000` per arm** is the two-proportion sample requirement at
  p0 = 0.08, p1 = 0.12, alpha 0.05 two-sided, power 0.8 (about 881), rounded up.
- **Pledges are open mutations.** Foundry actions hold no privileged token and
  write only to the program-owned `foundry` schema; every write is attributed in
  `foundry.action_log` and rate-capped per sid. A hostile visitor can skew an
  arm, and the action log is what makes that visible after the fact.
- **No simulated or synthetic record can enter this metric**, because none is
  written any more: the seeded program, the LCG bot swarm and the labeled
  simulated support all died with v3. Bot runs are ingested from real harness
  executions and emit no `fd_*` event at all — nothing a bot does can move a
  funnel here.
- `status: running` binds this metric against the 26 `fd_*` events in
  `events/foundry.ts`, so `telemetry:metrics:check` enforces the binding at full
  severity from the first deploy.
