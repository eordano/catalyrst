---
id: governance-submit-hiring
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking the Submit-Hiring proposal into explicit steps (choose add/remove ->
    pick the target committee + member -> reasons & evidence -> review -> submit)
    increases the share of started hiring submissions that reach the review step,
    even with createProposal stubbed.
  because: >-
    The single-page upstream form mixes a committee dropdown, a wallet address or
    member picker, two length-bounded markdown editors (reasons + evidence) and
    co-authors in one tall scroll gated behind a voting-power threshold; sequencing
    those into legible steps with a final review reduces uncertainty about what will
    be submitted, so more authors who start a hiring proposal push through to review
    instead of bailing mid-form.
metric:
  primary: gv_hiring_review_rate
  numerator: gv_hiring_review_reached
  denominator: gv_hiring_started
  guardrails:
    - gv_hiring_started
    - gv_hiring_target_invalid
    - gv_hiring_submit_error
experiment:
  key: gv_hiring_wizard
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
    Ship if gv_hiring_review_rate improves by at least the MDE with no guardrail
    regression (hiring-submission start volume holds, the invalid-target path stays
    graceful, and submit errors do not rise); otherwise hold.
---

# Submit a Hiring proposal -- add / remove committee member (simulated)

The Submit-Hiring wizard (`/governance/submit/hiring?request=add|remove`) breaks
the DAO's "Add / Remove Committee Member" proposal into explicit steps: pick the
target committee + the member (a wallet address when adding, a member dropdown
when removing) -> reasons & evidence (two markdown editors, 20..3000 chars) +
co-authors -> review -> submit -> success. The flow tracks whether the wizard
increases the share of started hiring submissions that reach the review step.

`?request` chooses the variant (add vs remove); an invalid value renders the
governance NotFound screen (GvNotFound), mirroring upstream `toHiringType`.

- **Primary metric:** `gv_hiring_review_rate` =
  `gv_hiring_review_reached` / `gv_hiring_started`.
- **Guardrails:** start volume (`gv_hiring_started`), the invalid-target path
  (`gv_hiring_target_invalid`) and submit failures (`gv_hiring_submit_error`)
  must stay healthy.
- **Events:** `gv_hiring_started` ({request}) on entry of the target step,
  `gv_hiring_target_submitted` ({request, committee}),
  `gv_hiring_target_invalid` ({request}) on the graceful invalid-target path,
  `gv_hiring_reasons_submitted` ({request, reasons_length, evidence_length, co_authors}),
  `gv_hiring_review_reached` ({request}),
  `gv_hiring_submitting` ({request}),
  `gv_hiring_submitted` ({request, proposal_id, stub: true}) on success, and
  `gv_hiring_submit_error` ({request}) on the simulated failure path.
- **Exposure:** `experiment_exposed` fires once when the wizard surface renders.

Data reality: governance is NOT a Catalyst service (the DAO runs on Snapshot +
Aragon externally), so the form copy/labels are verbatim from
decentraland/governance i18n (`page.submit_hiring.*`), the reasons/evidence bounds
and address/co-author rules are verbatim from `newProposalHiringScheme`, and the
committee list/membership comes from the `CommitteeName` enum
(`clients/Transparency.ts`, normally fed by `data.decentraland.vote/teams.json`,
which is not reachable from this app). The final `createProposal` is a
clearly-noted STUB -- no on-chain transaction or Snapshot proposal is created. The
flow, states and metrics are real.
