---
id: governance-vote-bid
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Forcing bid voters to reckon with the whole competing field (every Bid linked
    to the same tender) before they can confirm a choice increases the share of
    started bid votes that reach a deliberate cast, even with the cast stubbed.
  because: >-
    A Bid is only meaningful relative to its rivals inside the tender. Showing the
    sibling bids with budget + leading VP and gating the confirm behind an explicit
    "you have reviewed the field" acknowledgement reduces reflexive, context-free
    votes, so more voters who start push through to a considered cast rather than
    bailing or voting blind.
metric:
  primary: gv_bid_vote_cast_rate
  numerator: gv_bid_vote_cast_reached
  denominator: gv_bid_vote_started
  guardrails:
    - gv_bid_vote_started
    - gv_bid_vote_snapshot_redirect
experiment:
  key: gv_bid_vote_flow
  unit: session
  variants:
    - id: gated
      weight: 1
      flags:
        reckonGate: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_bid_vote_cast_rate improves by at least the MDE with no guardrail
    regression (bid-vote start volume holds and the Snapshot-redirect fallback
    rate does not spike); otherwise hold.
---

# Bid voting flow -- reckon with the field before casting (simulated)

Bid-type proposals get a dedicated voting flow instead of the standard voting
modal, because a Bid lives inside a *tender* with sibling competing bids. Before
a member confirms their vote, the flow makes them reckon with the whole field:
it lists every Bid linked to the same tender (budget + leading VP, the bid being
voted on rendered disabled), gates the choice behind an explicit acknowledgement,
then simulates the cast.

Route: `/governance/proposals/:id/bid-vote`. Journey steps are URL-addressable
via `?step`:

- `?step=review` -- the competing-bid field (showBudget + leadingVP), current bid disabled.
- `?step=choosing` -- pick a choice with the "must reckon with the field" gate.
- `?step=casting` -- simulated cast (deterministic resolver; stub).
- `?step=error` -- retry-in-{timer} on a failed cast.
- `?step=snapshot` -- Snapshot redirect fallback after repeated failures.

- **Primary metric:** `gv_bid_vote_cast_rate` = `gv_bid_vote_cast_reached` /
  `gv_bid_vote_started`.
- **Guardrails:** bid-vote start volume (`gv_bid_vote_started`) and the
  Snapshot-redirect fallback rate (`gv_bid_vote_snapshot_redirect`) must stay
  healthy.
- **Events:** `gv_bid_vote_started` (entering review), `gv_bid_vote_field_reviewed`
  (`{bids}` -- reckon gate satisfied, leaving review), `gv_bid_vote_choice_selected`
  (`{choice}`), `gv_bid_vote_cast_reached` (entering casting / cast confirmed),
  `gv_bid_vote_cast_failed` (`{attempt}`), `gv_bid_vote_snapshot_redirect`,
  `gv_bid_vote_completed` (`{choice}`, `stub: true`).

Data reality: the competing-bid field is REAL -- pulled from the live, no-auth
governance API (`governance.decentraland.org/api/proposals?type=bid`, filtered to
one tender, "SDK Support Team"), with a bundled fixture fallback. The cast itself
is **simulated/deferred**: governance is not a Catalyst service (the DAO runs on
Snapshot + Aragon externally) and there is no write API reachable from this app,
so casting is a clearly-stubbed deterministic resolver in the XState wizard. The
flow, states, the reckon gate, the retry/Snapshot-redirect fallback, and the
metrics are real.
