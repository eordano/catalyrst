---
id: governance-submit-council-veto
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking the Submit-Council-Decision-Veto proposal into explicit steps (paste
    the Council decision URL -> write the reasons to veto + optional suggestions ->
    add co-authors -> review -> submit) increases the share of started veto
    submissions that reach the review step, even with createProposal stubbed.
  because: >-
    The single-page upstream form mixes a validated Council Snapshot URL, a
    required length-bounded markdown rationale, an optional markdown suggestions
    block and a co-authors multiselect in one tall scroll behind a 2500-VP gate;
    sequencing those into legible steps with a final review reduces uncertainty
    about what will be submitted, so more authors who start a veto push through to
    review instead of bailing mid-form.
metric:
  primary: gv_council_veto_review_rate
  numerator: gv_council_veto_review_reached
  denominator: gv_council_veto_started
  guardrails:
    - gv_council_veto_started
    - gv_council_veto_url_invalid
    - gv_council_veto_submit_error
experiment:
  key: gv_council_veto_wizard
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
    Ship if gv_council_veto_review_rate improves by at least the MDE with no
    guardrail regression (veto-submission start volume holds, the invalid-URL path
    stays graceful, and submit errors do not rise); otherwise hold.
---

# Submit a Council Decision Veto proposal (simulated)

The Submit-Council-Decision-Veto wizard
(`/governance/submit/council-veto`) breaks the DAO's "Council Decision Veto"
proposal into explicit steps: paste the DAO Council Decision URL (validated as a
Council Snapshot link) -> write the required "Reasons to Veto" markdown plus an
optional "Suggestions to the Council" markdown -> add co-authors -> review ->
submit -> success. The flow tracks whether the wizard increases the share of
started veto submissions that reach the review step.

- **Primary metric:** `gv_council_veto_review_rate` =
  `gv_council_veto_review_reached` / `gv_council_veto_started`.
- **Guardrails:** start volume (`gv_council_veto_started`), the invalid-URL path
  (`gv_council_veto_url_invalid`) and submit failures
  (`gv_council_veto_submit_error`) must stay healthy.
- **Events:** `gv_council_veto_started` on first advance,
  `gv_council_veto_url_invalid` on the graceful invalid-URL path,
  `gv_council_veto_reasons_filled` ({reasons_length, has_suggestions}),
  `gv_council_veto_coauthors_set` ({coauthors}),
  `gv_council_veto_review_reached`,
  `gv_council_veto_submitting`,
  `gv_council_veto_submitted` ({proposal_id, stub: true}) on success, and
  `gv_council_veto_submit_error` ({error}) on the simulated failure path.
- **Exposure:** `experiment_exposed` fires once when the wizard surface renders.

Data reality: governance is NOT a Catalyst service (the DAO runs on Snapshot +
Aragon externally), so the form copy/fields/limits are derived verbatim from
decentraland/governance-ui (`src/pages/submit/council-decision-veto.tsx`,
`src/types/proposals.ts` `newProposalCouncilDecisionVetoScheme`,
`src/intl/en.json`), the Council-Snapshot URL check is SIMULATED (a pure
`parseDecisionUrl`, no remote Snapshot read), and the final
`createProposalCouncilDecisionVeto` is a clearly-noted STUB -- no on-chain
transaction or Snapshot proposal is created. The flow, the inline field
validation and the funnel telemetry are real.
