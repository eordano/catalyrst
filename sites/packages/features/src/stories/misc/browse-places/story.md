---
id: browse-places
status: running
owner: owner@example.com
hypothesis:
  statement: Surfacing live user_count and like_rate on each place card raises the card-click rate.
  because: Players gravitate to scenes that look active and well-rated; visible liveness and a high like percentage are strong social-proof signals that reduce the cost of deciding where to jump in.
metric:
  primary: place_card_clicked_rate
  numerator: place_card_clicked
  denominator: place_list_viewed
  guardrails:
    - place_list_viewed
decision:
  rule: Ship if place_card_clicked_rate improves by at least the MDE with no guardrail regression; otherwise hold.
experiment:
  key: browse-places-live-signals
  unit: session
  baseline: 0.18
  mde: 0.02
  min_sample: 4000
  variants:
    - id: live-signals
      weight: 1
      flags:
        showLiveCount: true
        showLikeRate: true
---

# Browse live places

The Places explorer is the front door of the explore family. This story tracks
whether the live-data treatment on each `PlaceCard` (real-time `user_count` and
`like_rate`, mapped via `mapPlace`) earns more card clicks than a card with no
social proof.

There is a single shipping variant (`live-signals`); the schema is kept fully
valid so the readout tooling, deterministic bucketing and flag-eval (`/flags` +
local hash) / catalyrst-telemetry wiring work unchanged when/if a control arm is
added later.

- **Primary metric:** `place_card_clicked_rate` = `place_card_clicked` / `place_list_viewed`.
- **Guardrails:** total `place_list_viewed` volume and bounce rate must not regress.
- **Events:** `place_list_viewed` on load, `place_card_clicked` on card click.
