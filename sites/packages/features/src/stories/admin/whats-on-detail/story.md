---
id: whats-on-detail
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Showing full event detail (host, schedule, description, coordinates) with a
    prominent JUMP IN CTA converts viewers into in-world launches.
  because: >-
    A complete, trustworthy event page answers "what, when, where, who" in one
    glance and pairs it with a single high-contrast JUMP IN action, so a larger
    share of detail views end in a launch click than a bare card would.
metric:
  primary: lp_event_jump_in_rate
  guardrails:
    - lp_event_viewed
    - lp_event_jump_in
experiment:
  key: lp_event_jump_in
  unit: session
  variants:
    - id: detail_cta
      weight: 1
      flags:
        prominentJumpIn: true
  baseline: 0.25
  mde: 0.05
decision:
  rule: >-
    Ship the detail + JUMP IN layout if lp_event_jump_in_rate (lp_event_jump_in /
    lp_event_viewed) clears the baseline with 95% confidence and no guardrail
    regresses; otherwise revise the CTA prominence.
---

# Story -- Event detail -> Jump In

`/whats-on/:id` is a simple browse/detail page (no XState -- the jump-in is a
single CTA, not a multi-step wizard). The loader fetches `/events/api/events/:id`
and dehydrates it; the ui3 `EventDetail` renders name, when (from `start_at`),
host (`user_name`), description, and coordinates.

- `lp_event_viewed` fires on load (`{ event_id, live }`).
- The JUMP IN CTA links to `event.url` and emits `lp_event_jump_in`
  (`{ event_id, position }`).
- An unknown id renders a graceful empty state (never crash).

Primary metric `lp_event_jump_in_rate = lp_event_jump_in / lp_event_viewed`.
