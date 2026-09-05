---
id: creator-metrics-funnel
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing the CREATOR funnel -- collection -> item -> publish ->
    curation-approval -> first sale -- joined to the creator's own sales lets us
    see exactly where creators drop off, so we can target the weakest step.
  because: >-
    The publish wizard already emits per-step events keyed by session id, and the
    marketplace Sales report is joinable by the same sid; rolling them into one
    funnel makes the largest drop-off legible instead of buried across two dapps.
metric:
  primary: creator_funnel_conversion
  guardrails: []
experiment:
  key: creator_metrics_funnel
  unit: session
  variants:
    - id: with-insights
      weight: 1
      flags:
        showInsights: true
  baseline: 0.16
  mde: 0.03
  min_sample: 3000
decision:
  rule: >-
    This is an instrumentation/dashboard story (single arm). Ship the insights
    dashboard if creator_funnel_viewed records non-zero traffic and the funnel
    counts stay monotonically non-increasing; use the readout to prioritise the
    step with the largest drop-off. No traffic split to evaluate.
---

# CREATOR funnel metrics: collection -> item -> publish -> curation -> sale

The Creator Insights dashboard rolls the CREATOR
publish funnel into one view and joins it to the creator's marketplace Sales:

- **Funnel stages** (counts rolled up from publish-wizard events, see
  `app/stories/create-publish-wizard/machine.ts` `PUBLISH_EVENTS`):
  Collection created (`ch_publish_started`) -> Item added
  (`ch_publish_destination_selected`) -> Published
  (`ch_publish_target_selected`) -> Curation submitted
  (`ch_publish_confirm_reached`) -> Curation approved
  (`ch_publish_completed`) -> First sale (`mk_first_sale`).
- **Sales join:** the marketplace Sales report (`MkMySalesHistory` STATS:
  total sales, total earnings, royalties) is joined per session by `sid`.

**Primary metric:** `creator_funnel_conversion` = `first_sale` / `collection_created`.
**Events:** `creator_funnel_viewed` on load (and on each `?stage` deep-link).

Data reality: the funnel/sales join is an analytics rollup (read back via
Metabase over catalyrst-telemetry), not a single Catalyst resource, so the
view-model is read from a local fixture (`app/fixtures/creator-metrics-funnel.json`,
with a `_source` note) through a tolerant zod schema. Stage counts, the
curation-approval stage, and the per-session sales join are SIMULATED; the
marketplace Sales stats mirror the real `MkMySalesHistory` shapes. Noted as
deferred.
