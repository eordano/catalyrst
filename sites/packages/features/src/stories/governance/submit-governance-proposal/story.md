---
id: governance-submit-governance-proposal
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, step-by-step "Submit a Governance proposal" wizard (intro VP-gate +
    linked-draft selector -> details -> co-authors -> review -> submit) increases
    the share of started Governance proposals that reach the submit/confirm step,
    versus the single long-scroll governance form with its eight stacked markdown
    bodies.
  because: >-
    The Governance Proposal is the densest DAO form -- a Title plus seven required
    markdown bodies behind a 2500 VP gate. Surfacing the VP gate and the linked
    Draft up front, then splitting the bodies / co-authors / review into legible
    steps, reduces drop-off from form fatigue, so more eligible proposers who
    start push through to submitting instead of abandoning the wall of fields.
metric:
  primary: gv_govprop_submit_rate
  numerator: gv_govprop_submit_attempted
  denominator: gv_govprop_started
  guardrails:
    - gv_govprop_started
    - gv_govprop_details_submitted
    - gv_govprop_vp_blocked
experiment:
  key: gv_govprop_wizard
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
    Ship if gv_govprop_submit_rate improves by at least the MDE with no guardrail
    regression (Governance-proposal start volume holds, details completion stays
    healthy, and the VP-gate block rate does not spike); otherwise hold.
---

# Submit a Governance proposal (multi-step write flow, simulated)

The Governance Proposal submit wizard (`/governance/submit/governance`) breaks the
DAO's densest submission -- formalizing a passed Draft into a binding Governance
Proposal -- into explicit, deep-linkable steps:

1. `?step=intro` -- the **>=2500 VP** submission-gate notice + the linked-**Draft**
   selector. The selector options are read **live** from the governance API
   (`governance.decentraland.org/api/proposals?type=draft`, passed Drafts), with a
   local fixture fallback. `START` is gated on VP.
2. `?step=details` -- Title + the **seven** required markdown bodies (Summary /
   Abstract / Motivation / Specification / Impacts / Implementation Pathways /
   Conclusion), rendered with ui3's config-driven `SubmitProposalForm`.
3. `?step=coauthors` -- optional co-authors (up to 5).
4. `?step=review` -- confirm the proposal.
5. `?step=submitting` -- **simulated** `createProposal` (stub).
6. `?step=success` -- success / outcome screen.

- **Primary metric:** `gv_govprop_submit_rate` = `gv_govprop_submit_attempted` /
  `gv_govprop_started`.
- **Guardrails:** start volume (`gv_govprop_started`), details completion
  (`gv_govprop_details_submitted`), and the VP-gate block rate
  (`gv_govprop_vp_blocked`) must stay healthy.
- **Events:** `gv_govprop_started` (`{linked_draft_id, vp}`), `gv_govprop_vp_blocked`
  (`{vp, threshold}`), `gv_govprop_details_submitted` (`{title_len, bodies_filled}`),
  `gv_govprop_details_invalid` (`{error_count}`), `gv_govprop_step_advanced`
  (`{to:"review"}`), `gv_govprop_submit_attempted`, `gv_govprop_submitted`
  (`{proposal_id, simulated:true}`), `gv_govprop_error`.

**Data reality / simulated:** Governance is NOT a Catalyst service -- the DAO runs
on Snapshot + Aragon externally and exposes no `createProposal` endpoint reachable
from this app. The linked-Draft selector READ is **live** from the governance API
(fixture fallback in `app/fixtures/governance-submit-governance-proposal.json`); the
final `createProposal` is **SIMULATED** in the XState machine (a stubbed resolver,
never a real POST/transaction). The flow, the VP gate, the field validation, and
the metrics are real; only the on-chain commit is a clearly-noted stub. The
proposer's voting power is sampled (above threshold by default; QA override via
`?vp=<n>`).
