---
id: landings-whatson-admin-moderate
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Framing What's On moderation as an explicit review wizard (queue -> review
    event -> decision -> confirm) increases the share of opened pending hangouts
    that reach a confirmed moderation decision, even with the admin commit
    stubbed.
  because: >-
    A legible per-event review flow with an explicit confirm step reduces
    accidental / ambiguous actions, so more moderators who open a pending hangout
    follow through to a deliberate approve / reject / feature decision instead of
    bouncing back to the queue undecided.
metric:
  primary: lp_whatson_admin_decision_confirm_rate
  numerator: lp_whatson_admin_moderation_confirmed
  denominator: lp_whatson_admin_event_opened
  guardrails:
    - lp_whatson_admin_event_opened
    - lp_whatson_admin_moderation_failed
experiment:
  key: lp_whatson_admin_moderation
  unit: session
  variants:
    - id: moderation_wizard
      weight: 1
      flags:
        moderation_wizard: true
  baseline: 0.5
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if lp_whatson_admin_decision_confirm_rate improves by at least the MDE
    with no guardrail regression (opened-event volume holds and the simulated
    moderation-failure path stays rare/graceful); otherwise hold.
---

# What's On admin moderation -- approve / reject / feature pending hangouts

The What's On admin moderation surface (`/landings/whatson-admin`) is the
admin-only review queue for community-submitted hangouts. It breaks moderation
into explicit steps: sign in (admin gate) -> review the pending queue -> open one
event -> choose a decision (approve / reject / feature / unfeature / archive) ->
confirm. This story tracks whether the wizard increases the share of opened
pending hangouts that reach a confirmed decision.

- **Primary metric:** `lp_whatson_admin_decision_confirm_rate` =
  `lp_whatson_admin_moderation_confirmed` / `lp_whatson_admin_event_opened`.
- **Guardrails:** opened-event volume (`lp_whatson_admin_event_opened`) and the
  simulated moderation-failure path (`lp_whatson_admin_moderation_failed`) must
  stay healthy.
- **Events:** `lp_whatson_admin_gate_viewed` (entering the gate),
  `lp_whatson_admin_authenticated` (sign in -- admin bearer SIMULATED),
  `lp_whatson_admin_queue_viewed` (queue), `lp_whatson_admin_event_opened`
  (`{event_id}`), `lp_whatson_admin_decision_made` (`{event_id, action}`),
  `lp_whatson_admin_moderation_confirmed` (`{event_id, action}`),
  `lp_whatson_admin_moderation_failed` (`{event_id, action, error}`),
  `lp_whatson_admin_moderated` (`{event_id, action, stub}`).

## Data reality (what is real vs simulated)

- **Real:** the pending queue is read from the LIVE catalyst list
  `GET /events/api/events` (`catalyrst-events` `get_event_list`), with a fixture
  fallback (`app/fixtures/landings-whatson-admin-moderate.json`, projected from
  the same live list). The moderation contract is the real
  `PATCH /events/api/events/{id}` `PatchEventBody` (named actions expand to
  overlay flags: approve -> `{approved:true,rejected:false}`; reject|archive ->
  `{approved:false,rejected:true}`; feature -> `{highlighted:true}`; unfeature ->
  `{highlighted:false}`). Flow, states, telemetry, and the request body are real.
- **Simulated / deferred:** the admin bearer. `patch_event` is gated by
  `authorize_admin` (`Authorization: Bearer <CATALYRST_EVENTS_ADMIN_TOKEN>`) and
  is **fail-closed (403)** for an anonymous browser session, so the `submitting`
  state runs a simulated PATCH (`simulateModerate`) that resolves the same
  `{id, local}` envelope the handler returns -- it never hits the network. The
  reject-reason note is carried in the simulated commit only (the handler does
  not yet persist `rejection_reason`).

Each journey step is URL-addressable via `?step` (`auth-gate`, `queue`,
`review-event`, `decision`, `submitting`, `moderated`) for screenshotting; deep
links hydrate the wizard via an XState snapshot with NO telemetry double-fire and
no auto-advancing simulated PATCH.
