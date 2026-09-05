---
id: whats-on
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A live-data What's On hub surfacing currently-live and upcoming events drives
    more visitors into event detail pages than a static landing.
  because: >-
    Real-time signals (a LIVE rail with attendee counts plus an upcoming grid)
    create urgency and relevance, so a larger share of feed loads convert into an
    event-card click-through than a generic, undated marketing page.
metric:
  primary: lp_whatson_event_ctr
  numerator: lp_event_card_clicked
  denominator: lp_whatson_viewed
  guardrails:
    - lp_whatson_viewed
    - lp_event_card_clicked
experiment:
  key: lp_whatson_feed
  unit: session
  variants:
    - id: live_feed
      weight: 1
      flags:
        liveRail: true
  baseline: 0.18
  mde: 0.05
decision:
  rule: >-
    Ship the live What's On feed if lp_whatson_event_ctr (lp_event_card_clicked /
    lp_whatson_viewed) clears the baseline with 95% confidence and no guardrail
    regresses; otherwise iterate on the rail/grid composition.
---

# Story -- Browse What's On (live + upcoming events feed)

The What's On hub (`/whats-on`) is a server-rendered, Catalyst-backed feed: a
LIVE rail (`list=live`) and an UPCOMING grid (`list=active`) from
`/events/api/events`, wrapped in ui3's `StWhatSOn`.

- Filters are URL-addressable: `?category=<name>` and `?search=<q>` re-query on
  the server, and re-emit `lp_whatson_viewed`.
- Each event card links to `/whats-on/:id` and emits `lp_event_card_clicked`.

Primary metric `lp_whatson_event_ctr = lp_event_card_clicked / lp_whatson_viewed`.
