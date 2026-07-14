---
id: creator-metrics
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a single CREATOR funnel (collection viewed -> item edited ->
    publish started -> curation -> published -> listed -> store viewed -> sale)
    plus headline counts in a Creator Hub Metrics tab helps creators see where
    they drop off and pushes more started publishes through to a completed sale.
  because: >-
    Creators today only see per-screen drill-down events scattered across
    Builder and Marketplace; rolling them into one persona funnel makes the
    leakiest step legible, so a dashboard view is expected to correlate with a
    higher publish-to-sale conversion rather than abandonment mid-flow.
metric:
  primary: creator_publish_to_sale_rate
  numerator: creator_sale_completed
  denominator: creator_publish_started
  guardrails:
    - creator_publish_started
    - creator_dashboard_viewed
experiment:
  key: creator_metrics_dashboard
  unit: session
  variants:
    - id: dashboard
      weight: 1
      flags:
        dashboard: true
  baseline: 0.08
  mde: 0.02
  min_sample: 6000
decision:
  rule: >-
    Ship if creator_publish_to_sale_rate (creator_sale_completed /
    creator_publish_started) improves by at least the MDE with no guardrail
    regression (publish-start volume holds and dashboard views stay healthy);
    otherwise hold.
---

# CREATOR funnel instrumentation + Creator Hub metrics dashboard

A new Creator Hub **Metrics** tab (`/creator-hub/metrics`) gives the CREATOR
persona the headline counts they care about: **published collections**,
**on-sale items**, **7-day sales**, and **scene visits (30d)** -- all scoped to
the connected wallet.

## Funnel

The persona funnel is emitted at the EXISTING Builder / Marketplace call sites,
ALONGSIDE (never instead of) the per-screen drill-down events. The rollups are
added by wrapping the injectable `track` sink each XState wizard already accepts
(`app/lib/telemetry/creator-funnel.ts`), so no machine -- or machine test -- is
touched.

- **Funnel-step events (ordered):** `creator_collection_viewed`,
  `creator_item_edited`, `creator_publish_started`,
  `creator_curation_submitted`, `creator_collection_published`,
  `creator_item_listed`, `creator_store_viewed`, `creator_sale_completed`.
- **Dashboard events:** `creator_dashboard_viewed` on mount.
- **Primary metric:** `creator_publish_to_sale_rate` =
  `creator_sale_completed` / `creator_publish_started` (read back via Metabase
  over catalyrst-telemetry, not on this page).

## Data reality (honestly creator-scoped -- NO fixtures)

Headline counts derive at request time from builder-server collections, the
marketplace catalog + `/market/v1/sales` (7-day rollup), and the Places API
(scene visits), each scoped to the connected wallet. There is **no fixture
fallback**: a source that fails renders that card as **"Not available"**
(`unavailable: true`) instead of fabricated numbers; when every source fails
the page shows the load-error banner + retry, and a wallet with nothing
published gets the EmptyState.

The rolled-up funnel PANEL was removed with the fixture purge: `creator_*`
events are sent client-side to catalyrst-telemetry and have **no SSR read-back
path** (the aggregate is read back via Metabase), so rendering a funnel here
would have meant fixture data. The funnel-step events themselves are real and
still emitted at the live call sites via `withCreatorFunnel` /
`trackCreatorFunnel` (`app/lib/telemetry/creator-funnel.ts`);
`creator_dashboard_funnel_clicked` is reserved for a future live read-back.
