---
id: governance-submit-linked-wearables
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking the Linked Wearables Registry proposal into explicit steps
    (project identity -> collection details -> technical setup -> review ->
    submit) increases the share of started submissions that reach the review
    step, even with createProposal stubbed.
  because: >-
    The upstream form is one very tall single page that mixes a name, a
    marketplace URL, repeatable link/image lists (up to 10 images), three
    length-bounded markdown blocks, an items count, two address lists, a
    programmatic-generation radio with a conditional Method field and a
    co-authors picker. Sequencing those into legible, individually-validated
    steps with a final review reduces uncertainty about what will be submitted,
    so more authors who start a Linked Wearables proposal push through to review
    instead of bailing mid-form.
metric:
  primary: gv_lw_review_rate
  numerator: gv_lw_review_reached
  denominator: gv_lw_started
  guardrails:
    - gv_lw_started
    - gv_lw_validation_error
    - gv_lw_submit_error
experiment:
  key: gv_lw_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.4
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_lw_review_rate improves by at least the MDE with no guardrail
    regression (Linked-Wearables submission start volume holds, per-step
    validation friction does not rise, and submit errors stay flat); otherwise
    hold.
---

# Submit a Linked Wearables Registry proposal (simulated)

The Submit-Linked-Wearables wizard
(`/governance/submit/linked-wearables`) breaks the DAO's "Linked Wearables
Registry" proposal -- upstream a single very tall form -- into explicit,
individually-validated steps:

1. `?step=identity` -- Name + NFT Marketplace Listing URL + Links list
2. `?step=collection` -- Collection Images (max 10) + NFT Collections markdown + items count
3. `?step=technical` -- Smart Contracts list + Managers list + a programmatically-generated Yes/No radio (revealing a conditional Method markdown field)
4. `?step=review` -- confirm everything before submit
5. `?step=submitting` -- simulated `createProposal` (stub)
6. `?step=success` -- success outcome screen

The flow tracks whether the wizard increases the share of started submissions
that reach the review step.

- **Primary metric:** `gv_lw_review_rate` =
  `gv_lw_review_reached` / `gv_lw_started`.
- **Guardrails:** start volume (`gv_lw_started`), per-step validation friction
  (`gv_lw_validation_error`) and submit failures (`gv_lw_submit_error`) must
  stay healthy.
- **Events:** `gv_lw_started` on first advance, `gv_lw_identity_filled`
  (`{links}`), `gv_lw_collection_filled` (`{images, items}`),
  `gv_lw_technical_filled` (`{contracts, managers, programmatic}`),
  `gv_lw_validation_error` (`{step, fields}`) on any step's graceful invalid
  self-loop, `gv_lw_review_reached` on entering review,
  `gv_lw_submitting` on submit, `gv_lw_submitted` (`{proposal_id, stub: true}`)
  on success, and `gv_lw_submit_error` (`{error}`) on the simulated failure path.
- **Exposure:** `experiment_exposed` fires once when the wizard surface renders.

Data reality: governance is NOT a Catalyst service (the DAO runs on Snapshot +
Aragon externally), so the form copy/fields/limits are derived verbatim from
decentraland/governance-ui (`src/pages/submit/linked-wearables.tsx`,
`src/types/proposals.ts` `newProposalLinkedWearablesScheme`, and
`src/intl/en.json`). The remote image-type validation is SIMULATED (a pure
https + extension check, no download), and the final
`createProposalLinkedWearables` is a clearly-noted STUB -- no on-chain
transaction or Snapshot proposal is created. The flow, the per-step validation
and the metrics are real.
