---
id: governance-submit-poll
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, multi-step poll submission wizard (intro/VP-gate -> details ->
    options -> review -> submit) increases the share of started poll submissions
    that reach the review/confirm step versus a single long form.
  because: >-
    Splitting the poll form into legible steps with inline validation (title
    5-80, description 20-7000, at least 2 options) and a final payload review
    reduces the chance an author abandons at an opaque single screen or bounces on
    a validation error, so more started submissions reach the confirm step and
    complete -- even with the on-chain/Snapshot write simulated.
metric:
  primary: gv_submit_poll_review_rate
  numerator: gv_submit_poll_review_reached
  denominator: gv_submit_poll_started
  guardrails:
    - gv_submit_poll_started
    - gv_submit_poll_vp_blocked
experiment:
  key: gv_submit_poll_flow
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 5000
decision:
  rule: >-
    Ship if gv_submit_poll_review_rate improves by at least the MDE with no
    guardrail regression (submission-start volume holds and the VP-gate /
    disconnected paths stay graceful, i.e. gv_submit_poll_vp_blocked does not
    spike from a broken gate); otherwise hold.
---

# Submit a community Poll (multi-step write flow, simulated)

The governance "Create a community poll" submission (`/governance/submit/poll`)
is broken into explicit, URL-addressable steps so an author can fill the form
without facing one opaque screen:

- `?step=intro` -- VP-gate notice (>= 100 VP required) plus a LogIn / connect-wallet
  gate when the wallet is disconnected.
- `?step=details` -- Title (5-80 chars) + Description markdown (20-7000 chars).
- `?step=options` -- poll options list (min 2, max 100) + optional co-authors
  (42-char addresses).
- `?step=review` -- confirm the payload + a Snapshot/Aragon note.
- `?step=submitting` -- simulated `createProposal` (deterministic resolver).
- `?step=success` -- `?new` success outcome screen.

Governance is **NOT** a Catalyst service -- Snapshot + Aragon run the DAO
externally and there is no write API -- so the submission is **SIMULATED**: the
machine's injectable `submitPoll` resolver returns a deterministic proposal ref
after a short delay (clearly noted stub). The flow, states, validation and
telemetry are real.

Schema/limits are transcribed verbatim from the upstream
`newProposalPollScheme` (title 5-80, description 20-7000, choices min 2 each
1-100 chars, co-authors 42-char addresses); the 100-VP requirement and copy come
from the governance `en.json`. Source recorded in
`app/fixtures/governance-submit-poll.json` (`_source`).

Flow / events:

- `gv_submit_poll_started {has_vp, connected}` -- leaving `intro` (NEXT).
- `gv_submit_poll_details_completed {title_len, description_len}` -- leaving `details`.
- `gv_submit_poll_options_completed {option_count, co_author_count}` -- leaving `options`.
- `gv_submit_poll_review_reached {option_count}` -- entering `review` (the primary).
- `gv_submit_poll_vp_blocked {connected}` -- the VP / connect gate blocked NEXT (guardrail).
- `gv_submit_poll_submitted {proposal_ref, stub: true}` -- the simulated submit resolves (`success`).

- **Primary metric:** `gv_submit_poll_review_rate` = `gv_submit_poll_review_reached` / `gv_submit_poll_started`.
- **Guardrails:** submission-start volume (`gv_submit_poll_started`) and the VP /
  connect gate (`gv_submit_poll_vp_blocked`) must stay healthy.

Data reality: there is no governance write API and no VP oracle wired here, so
the connected account + VP are seeded from the fixture and the final submit is a
clearly-noted simulation. Noted as deferred.
