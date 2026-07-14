---
id: governance-submit-poi
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, multi-step POI submission wizard (coordinates -> description ->
    review -> submit) increases the share of started POI proposals that reach the
    review/confirm step, even with the on-chain createProposal stubbed.
  because: >-
    Breaking the POI form into legible steps with inline coordinate and
    description validation reduces the chance a proposer abandons at an opaque,
    all-at-once form, so more of those who start reach the review step.
metric:
  primary: gv_poi_review_rate
  numerator: gv_poi_review_reached
  denominator: gv_poi_started
  guardrails:
    - gv_poi_started
    - gv_poi_coordinates_invalid
experiment:
  key: gv_poi_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_poi_review_rate improves by at least the MDE with no guardrail
    regression (POI-start volume holds and the coordinate-validation error rate
    does not spike); otherwise hold.
---

# Submit a Point of Interest proposal (add / remove, simulated)

The POI submission wizard (`/governance/submit/poi?request=add|remove`) walks a
proposer through choosing the request type, entering map coordinates, writing a
markdown description, reviewing, and submitting. This story tracks whether the
guided wizard increases the share of started POI proposals that reach the review
step.

Data reality: governance is NOT a Catalyst service -- the DAO runs on Snapshot +
Aragon and exposes no createProposal endpoint reachable from this app -- so the
final submission is **SIMULATED** (a clearly stubbed XState actor). The flow,
states, validation rules (coordinate ranges `x: -150..163`, `y: -150..159`,
description `20..250`, co-authors max 5) and funnel metrics are REAL, taken
verbatim from `decentraland/governance` `newProposalPOIScheme`.

- **Primary metric:** `gv_poi_review_rate` = `gv_poi_review_reached` / `gv_poi_started`.
- **Guardrails:** POI-start volume (`gv_poi_started`) and the coordinate
  validation error rate (`gv_poi_coordinates_invalid`) must stay healthy.
- **Events:** `gv_poi_started` (`{request}`) on entry, `gv_poi_coordinates_submitted`
  (`{x,y}`), `gv_poi_coordinates_invalid` (`{x,y}`), `gv_poi_description_submitted`
  (`{length,co_authors}`), `gv_poi_review_reached`, `gv_poi_submitting`,
  `gv_poi_submitted` (stub), `gv_poi_error`.

An invalid `?request` (anything but add/remove) renders `GvNotFound`, mirroring
the real product's `toPoiType -> null -> <NotFound />`.
