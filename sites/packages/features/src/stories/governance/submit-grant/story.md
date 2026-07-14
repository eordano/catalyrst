---
id: governance-submit-grant
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, step-by-step "Request a Grant" wizard (category -> funding ->
    general -> assessment -> review -> submit) increases the share of started
    grant requests that reach the submit/confirm step, versus the long
    single-scroll governance form.
  because: >-
    Splitting the dense grant request into legible steps -- with the live
    remaining-budget shown up front and funding tiers made explicit -- reduces
    drop-off from form fatigue, so more applicants who pick a category push
    through to submitting instead of abandoning the wall of fields.
metric:
  primary: gv_grant_submit_rate
  numerator: gv_grant_submit_attempted
  denominator: gv_grant_started
  guardrails:
    - gv_grant_started
    - gv_grant_funding_set
experiment:
  key: gv_grant_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.35
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_grant_submit_rate improves by at least the MDE with no guardrail
    regression (grant-request start volume holds and applicants still set a
    funding tier); otherwise hold.
---

# Request a Grant (multi-step write flow, simulated)

The grant request wizard (`/governance/submit/grant`) breaks the DAO's dense
"Request a Grant" submission into explicit, deep-linkable steps:

1. `?step=category` -- pick one of the seven VALID_CATEGORIES (suspended
   categories render disabled).
2. `?step=funding` -- desired budget + project duration; the matching funding
   tier and the **live remaining category budget** (read from upstream
   `governance.decentraland.org/api/budget/all`) are shown.
3. `?step=general` -- general info + team + due-diligence sections.
4. `?step=assessment` -- category-specific assessment + final consent.
5. `?step=review` -- confirm the request.
6. `?step=submitting` -- **simulated** `createProposal` (stub).
7. `?step=success` -- success / outcome screen.

- **Primary metric:** `gv_grant_submit_rate` = `gv_grant_submit_attempted` /
  `gv_grant_started`.
- **Guardrails:** grant-request start volume (`gv_grant_started`) and funding
  completion (`gv_grant_funding_set`) must stay healthy.
- **Events:** `gv_grant_started` (`{category}`), `gv_grant_funding_set`
  (`{tier,budget,duration}`), `gv_grant_step_advanced` (`{to}` for general /
  assessment / review), `gv_grant_submit_attempted`, `gv_grant_submitted`
  (`{proposal_id, simulated:true}`).

**Data reality / simulated:** Governance is NOT a Catalyst service -- the DAO
runs on Snapshot + Aragon externally and there is no grant-submission write API.
The remaining-budget read is LIVE from the governance API (fixture fallback in
`app/fixtures/governance-submit-grant.json`); the final `createProposal` is
SIMULATED in the XState machine (a stubbed resolver, never a real POST). The
flow, states, and metrics are real; only the on-chain commit is a clearly-noted
stub.
