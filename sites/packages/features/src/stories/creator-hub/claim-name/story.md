---
id: creator-hub-claim-name
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided in-context NAME claim flow launched from the publish-to-world empty
    state (enter name -> check availability -> review 100 MANA mint -> mint ->
    return with the NAME selectable) increases the share of creators who hit the
    empty-Worlds wall and then reach the mint-confirm/review step, even with the
    mint stubbed.
  because: >-
    A creator who reaches "Publish to a World" with no NAME is blocked: they need
    a NAME to get a World, but minting one bundles an unfamiliar ENS purchase, a
    100 MANA spend, and a return trip to the publish flow. Splitting it into
    explicit, legible, URL-addressable steps right where the block happens -- each
    making availability, cost, and the consequence clear before the irreversible
    mint -- reduces uncertainty, so more blocked creators push through to the
    review/confirm step instead of abandoning the publish.
metric:
  primary: ch_claim_name_review_rate
  numerator: ch_claim_name_review_reached
  denominator: ch_claim_name_started
  guardrails:
    - ch_claim_name_started
    - ch_claim_name_unavailable
experiment:
  key: ch_claim_name_wizard
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
    Ship if ch_claim_name_review_rate improves by at least the MDE with no
    guardrail regression (claim-start volume holds and the unavailable-name path
    stays graceful); otherwise hold.
---

# Claim a new Decentraland NAME for a World (from the publish-to-world empty state)

The Creator Hub publish wizard routes a creator to "Publish to a World", but a
wallet with no NAME hits the EmptyNames wall ("You don't have any available
World"). This story is the in-context claim flow launched from that wall
(`/creator-hub/claim-name`): it breaks minting a NAME into explicit,
URL-addressable steps and, on success, returns the creator to publish-to-world
with the freshly minted NAME selectable.

Steps (1:1 with the audit spec / machine states):
`name` (enter the desired NAME) -> `availability` (check availability + price)
-> `review` (confirm the 100 MANA mint) -> `mint` (simulated on-chain mint) ->
`done` (return to publish-to-world with the new NAME selectable).

- **Primary metric:** `ch_claim_name_review_rate` =
  `ch_claim_name_review_reached` / `ch_claim_name_started`.
- **Guardrails:** claim-start volume (`ch_claim_name_started`) and the
  unavailable-name path (`ch_claim_name_unavailable`) must stay healthy.
- **Events:** `ch_claim_name_started` (leaving the name step, carries the
  candidate name), `ch_claim_name_available` | `ch_claim_name_unavailable`,
  `ch_claim_name_review_reached` (the 100 MANA confirm step is shown),
  `ch_claim_name_mint_submitted` (stub tx), `ch_claim_name_completed` (stub --
  carries the new World name), `ch_claim_name_returned_to_publish` (the creator
  returns to publish-to-world with the NAME selectable).

## Consolidation (current serving reality)

`/creator-hub/claim-name` is deliberately a tracked **redirect**
(`creator_claim_name_redirect`, search params preserved) into the single
marketplace claim flow at `/marketplace/claim-name` -- the in-context wizard in
this directory is retained as the spec + machine/tests but is not routed. The
funnel handoff survives the consolidation: the deploy-world empty-NAMEs CTA
links with `?from=deploy-world`, the marketplace flow's success step then shows
**Use in Publish to World**, which fires `ch_claim_name_returned_to_publish`
and returns to `/creator-hub/deploy-world?name=<claimed>.dcl.eth`; deploy-world
preselects that NAME when (and only when) the wallet actually owns it -- deploys
there are real, so a not-yet-owned NAME is never preselected.

## Data reality (real vs simulated)

- **Real:** the existing-NAMEs context is LIVE catalyst
  (`/lambdas/users/{address}/names`), used to seed the wallet's owned NAMEs and
  the taken set so an already-owned NAME is excluded from availability. The NAME
  economics (100 MANA, Ethereum Mainnet, DCLRegistrar address, 2..15 alphanumeric
  validation) are protocol constants pinned from
  `decentraland/marketplace modules/ens/utils.ts`. The flow, states, telemetry,
  and validation are all real.
- **Simulated / deferred:** NAME minting is an on-chain write with no catalyst
  endpoint, so the DCLRegistrar `available(name)` read and the `register` mint
  are SIMULATED via the XState machine (read-only realm -- no on-chain writes).
  Availability is classified locally against the live owned-name set + a small
  known-taken list. The mint and final commit are clearly-noted stubs
  (`simulated: true` in the emitted props). The `done` step's "return to
  publish-to-world with the NAME selectable" is rendered via the ui3
  `ChPublishWizardPublishToWorld` selection screen (the real handoff back into
  the publish flow is the product behavior; here it is the screen, with the new
  NAME shown as a selectable World name).
