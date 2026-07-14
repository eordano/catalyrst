---
id: governance-submit-catalyst
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking the Submit-Catalyst-node proposal into explicit steps (choose
    add/remove -> node details -> rationale -> review -> submit) increases the
    share of started catalyst submissions that reach the review step, even with
    createProposal stubbed.
  because: >-
    The single-page upstream form mixes an Ethereum owner address, a domain with
    live server-status checks, a length-bounded markdown rationale and co-authors
    in one tall scroll; sequencing those into legible steps with a final review
    reduces uncertainty about what will be submitted, so more authors who start a
    catalyst proposal push through to review instead of bailing mid-form.
metric:
  primary: gv_catalyst_review_rate
  numerator: gv_catalyst_review_reached
  denominator: gv_catalyst_started
  guardrails:
    - gv_catalyst_started
    - gv_catalyst_domain_invalid
    - gv_catalyst_submit_error
experiment:
  key: gv_catalyst_wizard
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
    Ship if gv_catalyst_review_rate improves by at least the MDE with no
    guardrail regression (catalyst-submission start volume holds, the
    invalid-domain path stays graceful, and submit errors do not rise);
    otherwise hold.
---

# Submit a Catalyst node proposal (add / remove, simulated)

The Submit-Catalyst wizard (`/governance/submit/catalyst?request=add|remove`)
breaks the DAO's "Add / Remove a catalyst node" proposal into explicit steps:
node details (owner address + domain, with a simulated server-status check) ->
rationale (markdown) + co-authors -> review -> submit -> success. The flow tracks
whether the wizard increases the share of started catalyst submissions that reach
the review step.

`?request` chooses the variant (add vs remove); an invalid value renders the
governance NotFound screen (GvNotFound), mirroring upstream `toCatalystType`.

- **Primary metric:** `gv_catalyst_review_rate` =
  `gv_catalyst_review_reached` / `gv_catalyst_started`.
- **Guardrails:** start volume (`gv_catalyst_started`), the invalid-domain path
  (`gv_catalyst_domain_invalid`) and submit failures
  (`gv_catalyst_submit_error`) must stay healthy.
- **Events:** `gv_catalyst_started` ({request}) on first advance,
  `gv_catalyst_details_filled` ({request, already_a_catalyst}),
  `gv_catalyst_domain_invalid` ({request}) on the graceful invalid-domain path,
  `gv_catalyst_description_filled` ({request, length, coauthors}),
  `gv_catalyst_review_reached` ({request}),
  `gv_catalyst_submitting` ({request}),
  `gv_catalyst_submitted` ({request, proposal_id, stub: true}) on success, and
  `gv_catalyst_submit_error` ({request}) on the simulated failure path.
- **Exposure:** `experiment_exposed` fires once when the wizard surface renders.

Data reality: governance is NOT a Catalyst service (the DAO runs on Snapshot +
Aragon externally), so the form copy/fields are derived from
decentraland/governance-ui, the domain server-status check is SIMULATED (no
`/content/status` + `/lambdas/status` probe), and the final `createProposal` is a
clearly-noted STUB -- no on-chain transaction or Snapshot proposal is created. The
flow, states and metrics are real.
