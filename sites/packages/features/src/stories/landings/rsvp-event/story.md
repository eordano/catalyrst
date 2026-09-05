---
id: landings-rsvp-event
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A clear RSVP flow (sign-in gate -> confirm "going" -> confirmed, with a
    one-tap cancel) increases the share of started RSVPs that reach the "going"
    state, even with the auth signature stubbed.
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

# Attend / RSVP to an event (going) + cancel RSVP

The RSVP flow on an event landing (`/landings/rsvp-event`) lets an attendee mark
themselves **going** to a live What's On event and later **cancel** that RSVP.
Because the action is wallet-signed, the wizard breaks it into explicit steps: a
sign-in gate, a confirm step that states what you're committing to, a submitting
state, and a confirmed ("going") state with a one-tap cancel.

- **Primary metric:** `lp_rsvp_going_rate` = `lp_rsvp_going` / `lp_rsvp_started`.
- **Guardrails:** RSVP-start volume (`lp_rsvp_started`), cancel volume
  (`lp_rsvp_cancelled`), and the error/auth-rejected path (`lp_rsvp_error`) must
  stay healthy.
- **Events:** `lp_rsvp_started` (tap Going from idle), `lp_rsvp_signin`
  (auth-gate passed, simulated), `lp_rsvp_confirmed` (confirm step reached),
  `lp_rsvp_submitting`, `lp_rsvp_going` (`{event_id}`), `lp_rsvp_cancelling`,
  `lp_rsvp_cancelled` (`{event_id}`), `lp_rsvp_error` (`{reason}`).

## Data reality (simulated / deferred)

The attendee LIST + COUNT (GET `/events/api/events/{id}/attendees`) is LIVE and
unauthenticated, so the wizard seeds the roster/count from the real catalyst.
The RSVP **write** (POST) and **cancel** (DELETE) on the same path are LIVE in
catalyrst-events (`handlers/attendees.rs`) but auth-gated via a signed
**auth-chain** header (`require_signer`). The wizard does NOT mint a real
signature: the signed auth-chain header is **SIMULATED**. Flow, states, and
telemetry are real; the final RSVP commit/cancel is a clearly-noted stub that
updates the local count optimistically rather than hitting the auth-gated
endpoint.
