---
id: governance-delegate-vp
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided delegate-VP flow (browse delegates -> read a delegate's voting-power
    record -> confirm target -> sign the on-chain setDelegate) increases the share
    of started delegations that reach the confirm step.
  because: >-
    Delegating voting power is a high-trust decision that costs gas. Surfacing
    each delegate's live Snapshot voting power and archive activity before the
    confirm step reduces uncertainty, so more delegators who start the flow push
    through to confirm instead of bailing at an opaque one-shot "delegate" button.
metric:
  primary: gv_delegate_confirm_rate
  numerator: gv_delegate_confirm_reached
  denominator: gv_delegate_started
  guardrails:
    - gv_delegate_started
    - gv_delegate_candidate_viewed
experiment:
  key: gv_delegate_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.35
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_delegate_confirm_rate improves by at least the MDE with no
    guardrail regression (delegate-start volume holds and delegates are still
    being opened/read); otherwise hold.
---

# Delegate Voting Power on the Snapshot delegate registry

The delegate-VP flow (`/governance/delegate`) breaks delegation into explicit,
URL-addressable steps: browse the delegate roster, open a delegate's record
(live voting power broken down by strategy, archive vote count), confirm the
target, then sign. This story tracks whether the guided flow increases the share
of started delegations that reach the confirm step.

Delegation is an **on-chain `setDelegate(bytes32,address)` transaction sent from
the visitor's own wallet** to the Snapshot delegate registry. The site never
holds a key and never relays the call: the page builds the calldata, guards the
chain, and waits for the real receipt. Without
`SNAPSHOT_DELEGATE_CONTRACT_ADDRESS`/`SNAPSHOT_DELEGATE_CHAIN_ID` the sign step
fails closed with a named error instead of reporting a delegation that never
happened.

- **Primary metric:** `gv_delegate_confirm_rate` =
  `gv_delegate_confirm_reached` / `gv_delegate_started`.
- **Guardrails:** delegate-start volume (`gv_delegate_started`) and delegate
  engagement (`gv_delegate_candidate_viewed`) must stay healthy.
- **Events:** `gv_delegate_started` (entering the delegate detail with a chosen
  delegate), `gv_delegate_candidate_viewed` (`{candidate_id}`),
  `gv_delegate_confirm_reached` (`{candidate_id, vp}`), `gv_delegate_signing`
  (wallet transaction requested), `gv_delegate_completed`
  (`{candidate_id, vp, tx_hash, tx_status, chain_id}` -- `tx_status` is
  `confirmed` only when the receipt says so, `pending` otherwise).

Data reality: every number on the page is read, not authored. The roster is the
most active voters in the space over the last 180 days from the DAO vote archive
(`/votes/engagement` on catalyrst-governance) -- it is not a curated candidate
list and carries no profile copy, because no such source exists locally. Voting
power comes from the Snapshot hub `vp` query, and the visitor's current
delegation from an `eth_call` to the registry. Anything that cannot be read is
listed on the page as a blocker rather than filled in with a plausible number.
