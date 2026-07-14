---
id: governance-submit-pitch
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, multi-step Pitch submission wizard (VP-gate notice -> details ->
    co-authors -> review -> submit) increases the share of VP-eligible proposers
    who start a pitch and reach the review/confirm step, even with the on-chain
    createProposal stubbed.
  because: >-
    A Pitch is the first step in the Bidding & Tendering pipeline and asks for
    four long markdown sections at once; breaking it into legible steps with an
    explicit >=100 VP gate up front and inline length validation reduces the
    chance a proposer abandons at an opaque, all-at-once form, so more of those
    who start reach the review step.
metric:
  primary: gv_pitch_review_rate
  numerator: gv_pitch_review_reached
  denominator: gv_pitch_started
  guardrails:
    - gv_pitch_started
    - gv_pitch_details_invalid
experiment:
  key: gv_pitch_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.4
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_pitch_review_rate improves by at least the MDE with no guardrail
    regression (pitch-start volume holds and the details-validation error rate
    does not spike); otherwise hold.
---

# Submit a Pitch proposal (Bidding & Tendering, simulated)

The Pitch submission wizard (`/governance/submit/pitch`) walks a proposer through
the >=100 VP gate notice, the proposal details (initiative name + four markdown
sections: problem statement, proposed solution, target audience, relevance),
optional co-authors, a review, and submission. This story tracks whether the
guided wizard increases the share of started pitches that reach the review step.

A Pitch is the first step in the DAO's Bidding & Tendering pipeline: a validated
idea that can later spawn a Project Tender and external-team bids.

Data reality: governance is NOT a Catalyst service -- the DAO runs on Snapshot +
Aragon and exposes no createProposal endpoint reachable from this app -- so the
final submission is **SIMULATED** (a clearly stubbed XState actor). The flow,
states, validation rules (initiative_name `1..80`, the four markdown sections
`20..3500`, co-authors max 5 wallet addresses) and the >=100 VP submission gate
are REAL, taken verbatim from `decentraland/governance` `newProposalPitchScheme`
and `SUBMISSION_THRESHOLD_PITCH`.

- **Primary metric:** `gv_pitch_review_rate` = `gv_pitch_review_reached` / `gv_pitch_started`.
- **Guardrails:** pitch-start volume (`gv_pitch_started`) and the details
  validation error rate (`gv_pitch_details_invalid`) must stay healthy.
- **Events:** `gv_pitch_started` (`{meets_gate,vp}`) on entry, `gv_pitch_gate_passed`
  (intro -> details), `gv_pitch_details_submitted` (`{name_length,body_chars}`),
  `gv_pitch_details_invalid` (`{fields}`), `gv_pitch_coauthors_set` (`{count}`),
  `gv_pitch_review_reached`, `gv_pitch_submitting`, `gv_pitch_submitted` (stub,
  `{proposal_id}`), `gv_pitch_error`.

The journey is URL-addressable: each wizard step is deep-linkable via `?step`
(`intro|details|coauthors|review|submitting|success`, plus `error`). When the
connected account is below the 100 VP threshold the intro renders the VP-not-met
lockout (no gate-pass available), mirroring the real product's vpNotMet state.
