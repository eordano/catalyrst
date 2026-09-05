---
id: governance-home
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A metrics-forward DAO home that surfaces ending-soon proposals and treasury
    stats drives more visitors into the proposals list.
  because: >-
    Visitors who land on a bare governance hub bounce; leading with concrete,
    time-sensitive signals (proposals ending soon, votes this week, treasury
    size) gives an immediate reason to click through to the proposals list.
metric:
  primary: gv_home_viewed
  guardrails:
    - gv_proposals_clicked
experiment:
  key: gv_home_landing
  unit: session
  variants:
    - id: metrics-forward
      weight: 1
      flags:
        showMetrics: true
        showEndingSoon: true
  baseline: 0.25
  mde: 0.03
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_home_viewed sessions convert to gv_proposals_clicked at or above
    the MDE over baseline with no guardrail regression; otherwise hold.
---

# Governance home -- DAO landing

The DAO Home (`/governance`) is the front door of the governance hub. It is a
simple loader + component surface (no multi-step machine): the loader mints a
session id and emits the view event; the component composes ui3's
`GvHomeLanding` inside `GovernanceChrome`.

Governance is NOT a Catalyst service (the DAO uses Snapshot + Aragon), so the
ending-soon proposals and treasury/participation metrics are driven from the
local fixture `app/fixtures/governance.json` and clearly marked fixture/deferred.

- **Primary metric:** `gv_home_viewed` (home-landing views per session).
- **Guardrail / downstream:** `gv_proposals_clicked` (click-through to the list).
- **Events:** `gv_home_viewed` on load, `gv_proposals_clicked` on the
  "View all proposals" CTA.

Single shipping variant (`metrics-forward`); the schema stays fully valid so the
readout tooling and deterministic bucketing work unchanged if a control arm is
added later.
