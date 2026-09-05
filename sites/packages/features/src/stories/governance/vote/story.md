---
id: governance-vote
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided vote flow (choose choice -> optional rationale -> confirm) with a
    clear Snapshot fallback increases completed votes versus a single-click cast.
  because: >-
    A guided flow re-states the choice and voting power before committing and
    offers a rationale step, reducing accidental / low-confidence casts; a clear
    Snapshot fallback recovers votes that would otherwise be lost to dApp errors,
    so a larger share of started votes complete.
metric:
  primary: gv_vote_completed_rate
  guardrails:
    - gv_vote_started
    - gv_vote_snapshot_redirect
experiment:
  key: gv_vote_flow
  unit: session
  variants:
    - id: control
      weight: 50
      flags:
        guided: false
    - id: guided
      weight: 50
      flags:
        guided: true
  baseline: 0.55
  mde: 0.05
  min_sample: 6000
decision:
  rule: >-
    Ship the guided variant if gv_vote_completed_rate is higher than control with
    95% confidence and no guardrail (gv_vote_started volume,
    gv_vote_snapshot_redirect rate) regresses beyond tolerance; otherwise keep
    control.
---

# Governance vote -- cast a vote on a proposal (multi-step)

On a proposal detail page (`/governance/proposals/:id`) a connected member casts
a vote. This is a multi-step interactive journey, so it is driven by an XState
machine (copying the jump-in pattern) with per-transition telemetry; the visual
reuses ui3's `GvVoteCastingFlow`. Governance is NOT a Catalyst service, so the
actual cast is **stubbed** (no write API) -- the machine simulates success/failure
deterministically via an injectable resolver.

- **control** (`guided: false`): clicking Cast Vote casts immediately
  (`choosing -> casting`), no rationale step.
- **guided** (`guided: true`): `choosing -> reasoning` (optional rationale) ->
  `casting`; after repeated errors it routes to a `snapshotFallback` step.

Flow / events:

- `gv_vote_started {proposal_id, choice}` -- leaving `choosing` (both variants).
- `gv_vote_reasoned {proposal_id}` -- a rationale was added (guided only).
- `gv_vote_completed {proposal_id, choice}` -- the cast actor resolves
  (`registered`).
- `gv_vote_snapshot_redirect {proposal_id}` -- too many failures route to the
  Snapshot fallback.

- **Primary metric:** `gv_vote_completed_rate` = `gv_vote_completed` /
  `gv_vote_started`.
- **Guardrails:** `gv_vote_started` volume and `gv_vote_snapshot_redirect` rate.
