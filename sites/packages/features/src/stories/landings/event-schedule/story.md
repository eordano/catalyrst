---
id: landings-event-schedule
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step schedule builder (auth gate -> basics -> dates ->
    review) increases the share of started schedule drafts that reach the
    review/submit step, even with the moderator commit stubbed.
  because: >-
    Authoring a recurring program series is a moderator task with several fields
    (name, theme, colors, an active window). Breaking it into explicit, legible
    steps with per-step validation reduces uncertainty about what a schedule is
    and what is required, so more admins who start a draft push through to the
    review step instead of abandoning an opaque single-form editor.
metric:
  primary: lp_schedule_review_rate
  numerator: lp_schedule_review_reached
  denominator: lp_schedule_started
  guardrails:
    - lp_schedule_started
    - lp_schedule_submit_failed
experiment:
  key: lp_schedule_builder
  unit: session
  variants:
    - id: builder
      weight: 1
      flags:
        builder: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if lp_schedule_review_rate improves by at least the MDE with no
    guardrail regression (draft-start volume holds and the simulated submit-fail
    path stays rare); otherwise hold.
---

# Create / browse event schedules (recurring program series)

The event-schedule surface (`/landings/event-schedule`) lets moderators browse
the live schedule listing (recurring program series like Metaverse Fashion Week)
and build a new one through a guided wizard: an admin auth gate, the basics
(name, description, theme, background colors), the active date window, then a
review pass before the (simulated) commit.

- **Primary metric:** `lp_schedule_review_rate` = `lp_schedule_review_reached` /
  `lp_schedule_started`.
- **Guardrails:** draft-start volume (`lp_schedule_started`) and the simulated
  submit-fail path (`lp_schedule_submit_failed`) must stay healthy.
- **Events:** `lp_schedule_list_viewed` (browse), `lp_schedule_gate_viewed` on
  the auth gate, `lp_schedule_started` on leaving the gate,
  `lp_schedule_step_completed` (`{from,to}`) per forward step,
  `lp_schedule_review_reached` entering review, `lp_schedule_submit_attempted`
  on submit, `lp_schedule_submit_failed` (`{error}`) on a simulated error, and
  `lp_schedule_created` (stub) on success.

Data reality: `GET /events/api/schedules` is LIVE but currently returns an empty
list, so the browse rail falls back to fixture sample schedules. The create/edit
commit (`POST`/`PATCH /events/api/schedules`) requires a federation-signed
MODERATOR auth-chain and is fail-closed for an anonymous browser session, so the
admin auth + the final commit are SIMULATED via an XState machine. The
flow/states/metrics/draft-validation are real.
