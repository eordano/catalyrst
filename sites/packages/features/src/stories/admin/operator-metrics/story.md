---
id: operator-metrics
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving SCENE OPERATORS a single dashboard that pairs REAL live occupancy
    (peers, per-scene and per-world headcount from catalyrst-presence) with their
    own deploy funnel and ban/admin activity increases the share of operators who,
    after viewing occupancy, start a (re)deploy or a scene-admin action in the same
    session -- because seeing where players actually are makes the next operational
    step obvious.
  because: >-
    Operators today fly blind: deploy is a one-shot upload with no feedback on
    whether anyone is in the scene, and scene-admin/bans live on a separate surface.
    Surfacing live occupancy next to the deploy funnel and moderation activity turns
    the dashboard into the operator's home base, so an operator who notices a busy
    (or empty) scene is more likely to act on it (deploy an update, open scene-admin)
    instead of leaving.
metric:
  primary: operator_deploy_rate
  numerator: operator_deploy_completed
  denominator: operator_deploy_started
  guardrails:
    - operator_deploy_started
    - operator_placement_rejected
    - operator_visits_viewed
experiment:
  key: operator_metrics_dashboard
  unit: session
  variants:
    - id: dashboard
      weight: 1
      flags:
        dashboard: true
        occupancy: true
  baseline: 0.4
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if operator_deploy_rate (operator_deploy_completed / operator_deploy_started)
    improves by at least the MDE with no guardrail regression: deploy-start volume
    holds, the placement step keeps rejecting off-land / occupied placements
    gracefully (operator_placement_rejected stays healthy), and occupancy keeps
    rendering real numbers (operator_visits_viewed). Otherwise hold.
---

# SCENE-OPERATOR funnel instrumentation + occupancy/deploy dashboard

The operator dashboard (`/creator-hub/operator-metrics`) is the SCENE OPERATOR's
home base inside the Creator Hub chrome. It pairs **real live occupancy** with the
operator's own **deploy funnel** and **moderation activity** so the next
operational step (deploy an update, open scene-admin, issue a ban) is obvious:

- **Live occupancy (REAL):** read from **catalyrst-presence** --
  `GET /current` (the latest snapshot summary: `peers_count`, scene/world polled
  + user totals), `GET /current/scenes` (per-scene headcount, count desc), and
  `GET /current/worlds` (per-world membership). The presence service archives one
  snapshot per collection pass; the dashboard renders the most recent one.
- **Deploy funnel:** conversion through the `operator_*` deploy events emitted by
  the deploy-to-LAND and deploy-to-World wizards
  (`/creator-hub/deploy-land`, `/creator-hub/deploy-world`), alongside the
  existing `ch_deploy_*` story events.
- **Ban / admin activity:** scaffolded `operator_scene_admin_*` events for the
  future scene-admin surface (catalyst comms `/scene-admin`, `/users/{addr}/bans`).

Presence is not yet routed on the public catalyst edge
(`https://catalyst.example.com/presence/current` -> 404), so the reader
(`app/lib/catalyst/presence.ts` + `presence.server.ts`) falls back to the bundled
`app/fixtures/operator-metrics.json`. That fixture's **scene occupancy is REAL**
-- derived from `GET /places/api/places?order_by=most_active` (live 200:
`base_position` + `user_count` + `positions`) re-shaped into the presence
`SceneOccupancyRow` contract; **world occupancy is SYNTHETIC** (the places API
exposes no live per-world headcount -- the collector reads comms world rooms /
worlds-content-server `/live-data`). When presence is exposed, the live read drops
in unchanged.

- **Primary metric:** `operator_deploy_rate` =
  `operator_deploy_completed` / `operator_deploy_started`.
- **Guardrails:** deploy-start volume (`operator_deploy_started`), the
  placement-validation path (`operator_placement_rejected`), and occupancy
  rendering (`operator_visits_viewed`) must stay healthy.

## Events

Deploy funnel (emitted by the deploy-land + deploy-world wizards, alongside the
existing `ch_deploy_*` events):

- `operator_deploy_started` `{ target }` -- a publish/deploy flow opened
  (target: `land` | `world`).
- `operator_placement_validated` `{ target, base?, name? }` -- a valid placement /
  name selection (the destination is confirmed deployable).
- `operator_placement_rejected` `{ target, reason }` -- an invalid placement
  (`off-land` / `occupied`) or quota-exceeded bundle (guardrail).
- `operator_deploy_completed` `{ target, base?, name?, stub }` -- deploy reached
  the success screen (SIMULATED commit; `stub: true`).

Scene-admin / moderation (scaffolded for the future scene-admin surface; emitted
from the dashboard's instrumented links today):

- `operator_scene_admin_opened` `{ pointer? }` -- the scene-admin surface for a
  scene was opened.
- `operator_admin_changed` `{ pointer?, action }` -- an admin was added/removed
  on a scene (`action`: `add` | `remove`).
- `operator_ban_issued` `{ pointer?, address? }` -- a player was banned from a
  scene.

Dashboard:

- `operator_dashboard_viewed` -- the dashboard mounted.
- `operator_visits_viewed` `{ peers, scenes, worlds }` -- the occupancy snapshot
  rendered (the headline numbers from `/current`).
- `operator_dashboard_funnel_clicked` `{ target }` -- a funnel CTA (a deep link
  into a deploy / scene-admin surface) was clicked.

Edge cases stay graceful (never crash): an unreachable/empty presence service
falls back to the bundled fixture so occupancy always renders; the scene-admin
events are scaffolded (the surface is deferred) and the deploy commit is
SIMULATED (the deployer is read-only on this realm).
