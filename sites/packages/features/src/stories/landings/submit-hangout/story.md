---
id: landings-submit-hangout
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking "Submit a Hangout" into an explicit signed-in wizard (cover ->
    details -> location -> schedule -> review/preview -> submit) increases the
    share of started submissions that reach the submit step, versus the legacy
    single long form.
  because: >-
    A long all-at-once event form is intimidating and easy to abandon. Chunking
    it into small, legible steps with per-step validation reduces uncertainty
    and visible error surface, so more creators who start a hangout push through
    to submit instead of bailing mid-form.
metric:
  primary: lp_hangout_submit_rate
  numerator: lp_hangout_submit_attempted
  denominator: lp_hangout_started
  guardrails:
    - lp_hangout_started
    - lp_hangout_submit_failed
decision:
  rule: >-
    Ship if lp_hangout_submit_rate (submit-attempted / started) improves by at
    least the MDE with no guardrail regression (started volume holds and the
    simulated submit-failure rate stays flat); otherwise hold.
experiment:
  key: lp_hangout_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.35
  mde: 0.05
  min_sample: 4000
---

# Submit / edit a Hangout (authenticated multi-step wizard)

The Submit a Hangout flow (`/landings/submit-hangout`) breaks the legacy
single-page event form into a signed-in, URL-addressable wizard:
**sign-in gate -> cover -> details -> location -> schedule -> review -> preview ->
submitting -> submitted**. Each step validates only its own fields (upstream
`EventAttributes` bounds -- name/description length, coordinate range,
recurrence) before the wizard will advance.

- **Primary metric:** `lp_hangout_submit_rate` =
  `lp_hangout_submit_attempted` / `lp_hangout_started`.
- **Guardrails:** start volume (`lp_hangout_started`) and the simulated submit
  failure rate (`lp_hangout_submit_failed`) must stay healthy.
- **Events:** `lp_hangout_signin_gate_viewed` on the gate,
  `lp_hangout_started` on sign-in, `lp_hangout_step_completed` (`{from,to}`) per
  forward step, `lp_hangout_preview_opened`, `lp_hangout_submit_attempted`
  (`{recurrent,location}`), `lp_hangout_submit_failed` (`{error}`),
  `lp_hangout_submitted` (stub) on success.

## Data reality (simulated / deferred)

- **Categories are LIVE:** the form's category list is fetched from
  `GET /events/api/events/categories` (catalyrst-events), with the bundled
  `app/fixtures/landings-submit-hangout.json` as the SSR fallback.
- **Submit is SIMULATED:** `POST /events/api/events` (catalyrst-events
  `create_event`) is admin/auth-gated and **fail-closed** for an anonymous
  browser session, so the final commit never hits the network. The wizard's
  `submitting` state runs a simulated actor that validates the derived
  `CreateEventBody` (the exact contract the handler accepts -- name/description/
  start_at/finish_at/x/y) and resolves the same `{id, approved:false}` shape the
  real handler returns. Flow, states, validation and metrics are real; only the
  network write is stubbed. The form field shape mirrors upstream
  `decentraland/events` `EventAttributes`.
