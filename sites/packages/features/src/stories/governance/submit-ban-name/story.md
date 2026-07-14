---
id: governance-submit-ban-name
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, multi-step Ban Name submission wizard (name -> description ->
    review -> submit) increases the share of started ban-name proposals that
    reach the review/confirm step, even with the on-chain createProposal stubbed.
  because: >-
    Splitting the ban-name form into legible steps with inline name and
    description validation reduces the chance a proposer abandons at an opaque,
    all-at-once form, so more of those who start reach the review step.
metric:
  primary: gv_ban_name_review_rate
  numerator: gv_ban_name_review_reached
  denominator: gv_ban_name_started
  guardrails:
    - gv_ban_name_started
    - gv_ban_name_name_invalid
experiment:
  key: gv_ban_name_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_ban_name_review_rate improves by at least the MDE with no
    guardrail regression (ban-name-start volume holds and the name-validation
    error rate does not spike); otherwise hold.
---

# Submit a Ban Name proposal (simulated)

The Ban Name submission wizard (`/governance/submit/ban-name`) walks a proposer
through entering the name to ban, writing a markdown rationale (plus optional
co-authors), reviewing, and submitting. There is **no VP gate** on this proposal
type, matching the real product. This story tracks whether the guided wizard
increases the share of started ban-name proposals that reach the review step.

Data reality: governance is NOT a Catalyst service -- the DAO runs on Snapshot +
Aragon and exposes no createProposal endpoint reachable from this app -- so the
final submission is **SIMULATED** (a clearly stubbed XState actor that never
hits the network). The flow, states, validation rules and funnel metrics are
REAL, taken verbatim from `decentraland/governance` `newProposalBanNameScheme`:
name alphanumeric `^([a-zA-Z0-9]){2,15}$` (MIN_NAME_SIZE=2, MAX_NAME_SIZE=15),
description `20..250`, co-authors each a 42-char wallet address (max 5).

- **Primary metric:** `gv_ban_name_review_rate` = `gv_ban_name_review_reached` / `gv_ban_name_started`.
- **Guardrails:** ban-name-start volume (`gv_ban_name_started`) and the name
  validation error rate (`gv_ban_name_name_invalid`) must stay healthy.
- **Events:** `gv_ban_name_started` on entry, `gv_ban_name_name_submitted`
  (`{length}`), `gv_ban_name_name_invalid`, `gv_ban_name_description_submitted`
  (`{length,co_authors}`), `gv_ban_name_review_reached`, `gv_ban_name_submitting`,
  `gv_ban_name_submitted` (stub), `gv_ban_name_error`.

## Journey (URL-addressable)

- `/governance/submit/ban-name?step=details` -- name to ban (text field, no VP gate)
- `/governance/submit/ban-name?step=description` -- markdown rationale + co-authors
- `/governance/submit/ban-name?step=review` -- confirm
- `/governance/submit/ban-name?step=submitting` -- simulated createProposal (stub)
- `/governance/submit/ban-name?step=success` -- success outcome screen
