---
id: governance-submit-tender
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking the Submit-Tender proposal into explicit steps (review the linked
    Pitch + VP gate -> project details -> co-authors -> review -> submit)
    increases the share of started tender submissions that reach the review
    step, even with createProposalTender stubbed.
  because: >-
    The single-page upstream form stacks the read-only linked Pitch, a short
    Project name and five 3,500-character markdown sections (Summary, Problem
    statement, Technical specification, Use cases, Deliverables) plus a target
    quarter and co-authors into one tall scroll gated on 1000 VP; sequencing
    those into legible steps with a final review reduces uncertainty about what
    will be submitted, so more authors who start a tender push through to review
    instead of bailing mid-form.
metric:
  primary: gv_tender_review_rate
  numerator: gv_tender_review_reached
  denominator: gv_tender_started
  guardrails:
    - gv_tender_started
    - gv_tender_vp_gated
    - gv_tender_submit_error
experiment:
  key: gv_tender_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_tender_review_rate improves by at least the MDE with no guardrail
    regression (tender-submission start volume holds, the VP-gated path stays
    graceful, and submit errors do not rise); otherwise hold.
---

# Submit a Tender proposal (Bidding & Tendering, simulated)

The Submit-Tender wizard (`/governance/submit/tender?step=...`) breaks the DAO's
"Tender proposal" -- step two of the Bidding & Tendering process -- into explicit
steps: review the linked Pitch (read-only) behind a Voting-Power gate (>=1000 VP)
-> project details (name + Summary / Problem statement / Technical specification /
Use cases / Deliverables markdown + target quarter) -> co-authors -> review ->
submit -> success. The flow tracks whether the wizard increases the share of
started tender submissions that reach the review step.

`?linked_proposal_id=<id>` preselects the Pitch this Tender refines (mirroring
upstream `usePreselectedProposal`); it is preloaded read-only. The VP gate
mirrors `SUBMISSION_THRESHOLD_TENDER` (1000) -- below it the wizard surfaces the
`error.tender.submission_vp_not_met` notice and never advances.

- **Primary metric:** `gv_tender_review_rate` =
  `gv_tender_review_reached` / `gv_tender_started`.
- **Guardrails:** start volume (`gv_tender_started`), the VP-gated path
  (`gv_tender_vp_gated`) and submit failures (`gv_tender_submit_error`) must stay
  healthy.
- **Events:** `gv_tender_started` ({linked_proposal_id, voting_power}) on first
  advance past the parent/gate step, `gv_tender_vp_gated` ({voting_power,
  threshold}) on the graceful below-threshold path,
  `gv_tender_details_filled` ({linked_proposal_id, summary_len,
  target_release_quarter}), `gv_tender_coauthors_set` ({count}),
  `gv_tender_review_reached` ({linked_proposal_id}),
  `gv_tender_submitting` ({linked_proposal_id}),
  `gv_tender_submitted` ({linked_proposal_id, proposal_id, pending: true, stub:
  true}) on success, and `gv_tender_submit_error` ({linked_proposal_id}) on the
  simulated failure path.
- **Exposure:** `experiment_exposed` fires once when the wizard surface renders.

Data reality: governance is NOT a Catalyst service (the DAO runs on Snapshot +
Aragon externally), so the linkable Pitch list is pulled LIVE from the DAO
governance API (`GET /api/proposals?type=pitch`) and snapshotted into the
fixture; the form copy/limits are derived from decentraland/governance-ui, and
the final `createProposalTender` is a clearly-noted STUB -- no on-chain
transaction or Snapshot proposal is created. The success screen reproduces the
upstream `TenderPublishedModal` / `ProposalPendingModal` (`?pending` analog). The
flow, states and metrics are real.
