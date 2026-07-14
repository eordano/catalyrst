---
id: marketplace-activity
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a live, filterable recent-activity feed (settled sales, open
    listings, and bids) on the marketplace gives visitors social proof of a
    liquid market, raising the share of activity sessions that go on to open an
    asset detail versus a marketplace with no visible trade history.
  because: >-
    Buyers hesitate when a market looks dead; showing real, recent sales with
    MANA prices and counterparties -- and letting visitors narrow to the kind of
    activity they care about (sale / listing / bid) -- signals liquidity and
    lowers the perceived risk of transacting, lifting feed-to-detail intent.
metric:
  primary: mk_activity_feed_to_detail conversion
  numerator: mk_activity_row_clicked
  denominator: mk_activity_viewed
  guardrails:
    - mk_activity_filter_applied
    - mk_activity_paginated
    - mk_activity_row_clicked
experiment:
  key: marketplace_activity
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.12
  mde: 0.03
decision:
  rule: >-
    Ship the live activity feed if the feed-to-detail conversion
    (mk_activity_row_clicked sessions / mk_activity_viewed sessions) beats the
    no-feed baseline with 95% confidence and no guardrail regresses; otherwise
    iterate on the row layout and default filter.
---

# Marketplace -- Activity / recent sales & trades feed

A server-rendered, Catalyst-backed feed of recent marketplace activity, wrapped
in ui3's `MarketplaceChrome`. Because `/credits/v1/activity` 400s without an Auth
Chain, the feed is composed from two LIVE endpoints and normalized onto one
chronological list:

- `GET /credits/v1/sales` -- settled sales (mint / order / bid), `{data,total}`.
- `GET /credits/v1/trades` -- open/signed trades (listing / bid), nested
  `{ ok, data: { data, count } }`. This endpoint is ~46MB unpaginated, so it is
  **always first-limited** (clamped to `TRADES_HARD_CAP`); we never fetch it
  whole.

Journey (each step is URL-addressable for screenshotting):

- `/marketplace/activity` loads the SSR feed -> `mk_activity_viewed { count, type, page }`.
- `?type=sale|listing|bid` narrows the feed (sales filter on the server via the
  `type` query; listings/bids derive from the trade kind) ->
  `mk_activity_filter_applied { type }`.
- `?page=N` (or `?skip=`) pages the feed; sales paginate on the server, trades
  are first-limited per page -> `mk_activity_paginated { page, direction }`.
- Clicking a row links to the asset/explorer -> `mk_activity_row_clicked { id, kind }`.

There is a single `default` variant (no A/B split) -- this is a "spec"-priority
browse surface (loader + components), not an interactive wizard. The experiment
`key` is wired into flag-eval (`/flags` + local hash) and catalyrst-telemetry so
the feed can later be gated/measured without code changes.

Emitted events: `mk_activity_viewed`, `mk_activity_filter_applied`,
`mk_activity_paginated`, `mk_activity_row_clicked`.

## Simulated / deferred

The feed is read-only. There is no buy/list/bid commit from this surface (those
live in their own wizard stories) -- clicking a row navigates to the existing
`/marketplace/:id` asset detail, which surfaces the "buying coming soon" state.
