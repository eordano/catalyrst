---
id: landings-rsvp-event
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A clear RSVP flow (sign-in gate -> confirm "going" -> confirmed, with a
    one-tap cancel) increases the share of started RSVPs that reach the "going"
    state through a real signed attendee write.
  because: >-
    Making the steps explicit (who you are -> what you're committing to ->
    confirmation) reduces uncertainty about a wallet-signed action, so more
    attendees who tap "Going" push through the confirm step instead of bailing
    at an opaque single-shot signature prompt.
metric:
  primary: lp_rsvp_going_rate
  guardrails:
    - lp_rsvp_started
    - lp_rsvp_cancelled
    - lp_rsvp_error
experiment:
  key: lp_rsvp_confirm
  unit: session
  variants:
    - id: confirm
      weight: 1
      flags:
        confirmStep: true
  baseline: 0.55
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if lp_rsvp_going_rate improves by at least the MDE with no guardrail
    regression (RSVP-start volume holds, cancel rate stays flat, and the
    error/auth-rejected path stays graceful); otherwise hold.
---

# Attend an event or cancel an RSVP

## Current capability

The production route reads event/attendee data and injects `buildRsvpCommit`.
Going and cancellation are real signed POST/DELETE requests to
`/events/api/events/{id}/attendees`. The commit reads the current identity when
invoked, so a sign-in after mounting works and a sign-out refuses subsequent
writes. The sign-in button opens the shared authentication dialog and advances
only when authentication succeeds.

The production result follows the endpoint acknowledgement. A returned total
updates the count; without a usable count the previous count is retained rather
than inventing a new one. Missing identity/event and request failures do not
produce a confirmed RSVP. Standalone preview machines can still explicitly use
`simulateCommit`; telemetry marks those results as stubs, while injected real
commits emit `stub: false`.

## Assumptions and measurement

RSVP is intent to attend, not proof of attendance. Public attendee reads do not
establish the current user's membership. The primary is `lp_rsvp_going` divided
by `lp_rsvp_started`; preserve cancel and error counts as guardrails. This is a
single-arm draft, so it cannot establish lift without a comparison.
Use the shared [Events assumptions](../../../../../../docs/product-capabilities.md#events-profiles-and-community-landings).
