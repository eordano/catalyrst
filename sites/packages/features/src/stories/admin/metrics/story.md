---
id: admin-metrics
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A single cross-surface moderation-metrics dashboard (open-queue depth,
    decisions, approval split, median time-to-decision across places /
    communities / events) raises the share of admin sessions that drill into a
    specific moderation surface instead of bouncing off scattered, per-service
    admin tools.
  because: >-
    Surfacing queue depth and SLA side-by-side makes the most backed-up surface
    legible at a glance, so admins who view the dashboard are more likely to act
    on the worst queue (a surface deep-link click) than when each backlog lives
    behind its own opaque tool.
metric:
  primary: admin_metrics_surface_click_rate
  numerator: admin_metrics_surface_clicked
  denominator: admin_metrics_viewed
  guardrails:
    - admin_metrics_viewed
experiment:
  key: admin_moderation_metrics
  unit: session
  variants:
    - id: dashboard
      weight: 1
      flags:
        dashboard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if admin_metrics_surface_click_rate (sessions with a
    admin_metrics_surface_clicked / sessions with admin_metrics_viewed) improves
    by at least the MDE with no guardrail regression (dashboard view volume
    holds); otherwise hold.
---

# Admin -- Moderation metrics dashboard

A read-only, server-rendered dashboard (`/admin/metrics`) that aggregates the
moderation workload across the three surfaces an ADMIN owns:

- **Places reports** -- catalyrst-places `GET /api/reports` (status
  `open|resolved|dismissed|actioned`, `count_reports`); deep-links to the
  places moderation queue.
- **Communities** -- catalyrst-communities `GET /v1/admin/communities`
  (`active|suspended|inactive`) + the suspension backlog
  `GET /v1/moderation/communities`; deep-links to communities moderation.
- **What's On events** -- catalyrst-events `GET /events/api/events` bucketed via
  `bucketOf` (`pending|approved|rejected|featured`); deep-links to
  `/landings/whatson-admin`.

The page renders one tile per figure, plus deep links into the three
moderation consoles. There is no wizard: loader -> plain data -> render.

- **Primary metric:** `admin_metrics_surface_click_rate` =
  `admin_metrics_surface_clicked` sessions / `admin_metrics_viewed` sessions.
- **Guardrails:** dashboard view volume (`admin_metrics_viewed`).
- **Events:**
  - `admin_metrics_viewed` `{ live_tiles, unavailable_tiles }` -- once on load.
  - `admin_metrics_surface_clicked` `{ surface }` -- deep-links into the matching
    moderation console.

## Data reality -- the fixture does not ship as numbers

`src/fixtures/admin-metrics.json` says in its own `_source` field that its
counts are synthetic. Rendered inside admin chrome -- KPI cards, a sparkline, a
funnel -- they read as production telemetry. So they are not rendered.

What ships:

- **Two live counts**, approved and featured events, from
  `GET /events/api/events?list=all`. That endpoint is public
  (`catalyrst-events/src/handlers/events.rs:345-362`, `optional_user`, no gate),
  and the tiles are labelled "live - public events API".
- **Everything else is an explicit empty state carrying its reason.** Not a
  zero, not a dash with a greyed chart: both still read as a measurement. The
  KPI table, the decision trend and the moderation funnel each render an
  unavailable notice, because this node has no moderation-aggregation endpoint.

The fixture is not deleted. It is reachable only through
`loadSampleAdminMetrics()`, which is off by default, is called from no loader,
and returns `synthetic: true` plus a mandatory banner string for layout work.

`operator-metrics.server.ts` is deliberately **not** used as a substitute. It is
creator-hub-owned, and it serves aggregate telemetry to any visitor with no
authorization at all -- that is a finding to report, not a data source to adopt.

The 7d/30d range toggle is gone, and `admin_metrics_range_changed` with it.
Nothing on the page is windowed, so it was a control over data that does not
exist.
