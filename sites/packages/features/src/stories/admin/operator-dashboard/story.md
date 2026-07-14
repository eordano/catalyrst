---
id: operator-dashboard
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving Scene Operators a single dashboard with live visits, a per-place
    headcount trend, and moderation load (bans + admins) per operated place
    increases how often they act on their scenes -- opening a place, adjusting
    moderation, or changing the time range -- instead of leaving the explorer.
  because: >-
    Operators today have no consolidated view of their portfolio: live player
    counts, 24h visit volume, like_rate and moderation load are scattered across
    the places explorer, the comms scene-admin tools, and the disable flow.
    Surfacing all of it on one SSR page -- ranked by visits, with a sparkline and
    deep-links into the per-place ban/admin surfaces -- turns a passive check-in
    into an actionable session and should raise card clicks and moderation-link
    follow-through.
metric:
  primary: operator_dashboard_viewed
  guardrails:
    - operator_place_card_clicked
    - operator_dashboard_moderation_link
experiment:
  key: operator_dashboard
  unit: session
  variants:
    - id: with-dashboard
      weight: 1
      flags:
        showOperatorDashboard: true
  baseline: 0.12
  mde: 0.03
  min_sample: 5000
decision:
  rule: >-
    Ship if operator_dashboard_viewed sessions are non-zero and at least 8% of
    viewers fire either operator_place_card_clicked or
    operator_dashboard_moderation_link; hold if the moderation-link rate is zero
    (deep-links not rendering) or range changes never fire (the control is dead).
---

# Scene Operator -- Operator dashboard

The operator dashboard (`/operator/dashboard`) is the Scene Operator persona's
control surface. It rolls the operator's portfolio (places filtered by
`?owner=`) into one SSR-rendered, no-JS-required view with three layers:

1. **Per-place KPI cards** (`OperatorPlaceSummary`) -- live players
   (`user_count`), 24h visits, `like_rate`, banned count, admin count, and the
   soft-disabled state for each operated place.
2. **Headcount trend** (`HeadcountTrend`) -- a presence
   `/scenes/history?pointer=x,y` sparkline per place, windowed by the `?range`
   control (1h / 6h / 24h).
3. **Visit ranking + moderation load** (`PlaceVisitTable`,
   `ModerationLoadCard`) -- operated places ranked by 24h visits (reusing the
   `au-*` table CSS) and per-place ban/admin cards that deep-link into the
   `operator-scene-bans` / `operator-scene-admins` surfaces for that place.

## Data sources

- **catalyrst-places** `GET /places/api/places?owner=<addr>` -- operated
  `PlaceRow` stats (`user_count` live players, `user_visits`, likes/dislikes,
  favorites, `like_rate`, `highlighted`, `disabled`). This is the only **LIVE**
  path. On the current deployment the squid owner pool is unconfigured, so
  `?owner=` returns the unfiltered list; the loader pins the operator's
  portfolio by id (the fixture) and overlays live `PlaceRow` stats per id.
- **catalyrst-presence** `GET /current`, `/current/scenes`, `/scenes/history`
  -- per-scene live occupancy + headcount time-series
  (`SceneOccupancyRow { taken_at, pointer, count }`). presence is **NOT exposed
  on the public gateway** (`/presence/*` -> 404), so the trend sparklines are
  **SYNTHESIZED** in the fixture from each place's live `user_count`.
- **catalyrst-comms** `GET /scene-bans`, `/scene-admin` -- moderation load per
  place. Both are signed-fetch gated (`decentraland-kernel-scene` signer) and
  unreachable from an anonymous SSR loader, so `banned_count` / `admin_count`
  are **SIMULATED** from the fixture.

Route: `/operator/dashboard`. URL-addressable: `?range=1h|6h|24h` selects the
sparkline window (server-rendered each navigation).

## Events

- `operator_dashboard_viewed` `{ place_count, total_live_players }` -- on mount
  (the primary impression metric).
- `operator_place_card_clicked` `{ place_id }` -- click on a per-place KPI card
  (opens the place in the explorer).
- `operator_dashboard_range_changed` `{ range }` -- the 1h / 6h / 24h range
  toggle (drives `?range`).
- `operator_dashboard_moderation_link` `{ place_id, target }` --
  `target: "scene-bans" | "scene-admins"`; click on a moderation deep-link in a
  `ModerationLoadCard`.
