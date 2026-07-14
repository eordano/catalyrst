---
id: governance-submit-draft
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, step-by-step "Submit a Draft proposal" wizard (intro/VP-gate ->
    details -> co-authors -> review -> submit) increases the share of started
    Draft submissions that reach the submit/confirm step, versus the long
    single-scroll governance Draft form.
  because: >-
    Splitting the dense Draft proposal -- five long markdown bodies plus the
    1000-VP gate, a linked-poll selector and co-authors -- into legible steps with
    the VP requirement surfaced up front reduces drop-off from form fatigue, so
    more authors who start a Draft push through to submitting instead of
    abandoning the wall of fields.
metric:
  primary: gv_draft_submit_rate
  numerator: gv_draft_submit_attempted
  denominator: gv_draft_started
  guardrails:
    - gv_draft_started
    - gv_draft_details_completed
experiment:
  key: gv_draft_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.3
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_draft_submit_rate improves by at least the MDE with no guardrail
    regression (Draft-start volume holds and authors still complete the details
    step); otherwise hold.
---

# Submit a Draft proposal (multi-step write flow, simulated)

The Draft proposal wizard (`/governance/submit/draft`) breaks the DAO's dense
"Draft proposal" submission into explicit, deep-linkable steps:

1. `?step=intro` -- VP-gate notice (**>= 1000 VP** required) + LogIn gate +
   **linked-poll selector** (a Draft must link a PASSED Poll; the poll list is
   read LIVE from upstream `governance.decentraland.org/api/proposals?type=poll`).
2. `?step=details` -- Title + the five markdown bodies (Summary / Abstract /
   Motivation / Specification / Conclusion).
3. `?step=coauthors` -- co-authors (optional).
4. `?step=review` -- confirm the proposal.
5. `?step=submitting` -- **simulated** `createProposal` (stub).
6. `?step=success` -- success / outcome screen.

- **Primary metric:** `gv_draft_submit_rate` = `gv_draft_submit_attempted` /
  `gv_draft_started`.
- **Guardrails:** Draft-start volume (`gv_draft_started`) and details completion
  (`gv_draft_details_completed`) must stay healthy.
- **Events:** `gv_draft_started` (`{poll_id}` -- fired when the VP/LogIn gate is
  cleared and a poll is selected), `gv_draft_details_completed`
  (`{title_len, bodies}`), `gv_draft_coauthors_set` (`{count}`),
  `gv_draft_step_advanced` (`{to}` for review), `gv_draft_submit_attempted`,
  `gv_draft_submitted` (`{proposal_id, simulated:true}`).

**Data reality / simulated:** Governance is NOT a Catalyst service -- the DAO runs
on Snapshot + Aragon externally and there is no Draft-submission write API. The
linkable-poll read is LIVE from the governance API (fixture fallback in
`app/fixtures/governance-submit-draft.json`); the final `createProposal` is
SIMULATED in the XState machine (a stubbed resolver, never a real POST). The
>= 1000 VP gate and the LogIn gate are real gating states (account/VP values are
sample/simulated). The flow, states, and metrics are real; only the on-chain
commit is a clearly-noted stub.
