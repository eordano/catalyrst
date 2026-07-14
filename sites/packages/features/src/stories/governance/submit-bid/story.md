---
id: governance-submit-bid
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, step-by-step "Submit Bid proposal" wizard -- parents (read-only
    Pitch + Tender) -> funding -> general -> review -> submit -- increases the
    share of started bids that reach the submit/confirm step, versus the long
    single-scroll Bidding & Tendering form.
  because: >-
    A Bid descends from a passed Tender and Pitch, so applicants arrive with
    context already established; surfacing the parent chain up front and
    splitting the dense bid form into legible steps reduces drop-off from form
    fatigue, so more teams who open a bid push through to submitting instead of
    abandoning the wall of funding/team/due-diligence fields.
metric:
  primary: gv_bid_submit_rate
  numerator: gv_bid_submit_attempted
  denominator: gv_bid_started
  guardrails:
    - gv_bid_started
    - gv_bid_funding_set
experiment:
  key: gv_bid_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.33
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_bid_submit_rate improves by at least the MDE with no guardrail
    regression (bid-start volume holds and applicants still complete the funding
    step); otherwise hold.
---

# Submit a Bid proposal (Bidding & Tendering, simulated)

The bid submission wizard (`/governance/submit/bid`) breaks the DAO's dense
"Submit Bid proposal" form into explicit, deep-linkable steps:

1. `?step=parents` -- the parent **Pitch** + **Tender** this Bid descends from,
   shown as read-only cards. These are **LIVE** passed proposals fetched from
   `governance.decentraland.org/api/proposals?type=tender` (+ the linked pitch).
2. `?step=funding` -- budget (100-240,000 USD) + project duration (1-12 months),
   delivery date, beneficiary address, contact email.
3. `?step=general` -- general info (team name, deliverables, roadmap, milestones),
   team members, due-diligence budget breakdown, and the final consent markdown.
4. `?step=review` -- confirm the bid.
5. `?step=submitting` -- **simulated** `createProposal` (stub).
6. `?step=success` -- the `BidSubmittedModal` "submitted but not published"
   pending outcome (the `?bid` analog of the proposal-detail success screens).

- **Primary metric:** `gv_bid_submit_rate` = `gv_bid_submit_attempted` /
  `gv_bid_started`.
- **Guardrails:** bid-start volume (`gv_bid_started`) and funding completion
  (`gv_bid_funding_set`) must stay healthy.
- **Events:** `gv_bid_started` (`{tender_id}`), `gv_bid_funding_set`
  (`{budget,duration}`), `gv_bid_step_advanced` (`{to}` for general / review),
  `gv_bid_submit_attempted`, `gv_bid_submitted`
  (`{proposal_id, published:false, simulated:true}`).

**Data reality / simulated:** Governance is NOT a Catalyst service -- the DAO runs
on Snapshot + Aragon externally and there is no bid-submission write API. The
parent Pitch + Tender cards are LIVE from the governance API (captured in
`app/fixtures/governance-submit-bid.json` with a `_source` URL; a tolerant zod
fallback keeps the route rendering on drift). The funding range, project
duration, and field limits are verbatim from
`decentraland/governance@master src/entities/Bid/types.ts`. The final
`createProposal` is SIMULATED in the XState machine (a stubbed resolver, never a
real POST); per the real flow the bid resolves as "submitted but not published"
until a competing bid exists. The flow, states, and metrics are real; only the
on-chain commit is a clearly-noted stub.
