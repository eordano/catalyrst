---
id: marketplace-claim-name
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step claim flow (enter name -> check availability -> approve
    MANA -> confirm -> mint) increases the share of started NAME claims that
    reach the on-chain confirm step.
  because: >-
    Minting a NAME bundles an unfamiliar ENS purchase with a MANA approval and a
    100 MANA spend; splitting it into explicit, legible steps (each making the
    cost, network, and approval consequence clear before the irreversible mint)
    reduces uncertainty, so more claimers push through to confirm instead of
    bailing at an opaque single-shot purchase.
metric:
  primary: mk_claim_name_confirm_rate
  numerator: mk_claim_name_confirm_reached
  denominator: mk_claim_name_started
  guardrails:
    - mk_claim_name_started
    - mk_claim_name_unavailable
experiment:
  key: mk_claim_name_wizard
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
    Ship if mk_claim_name_confirm_rate improves by at least the MDE with no
    guardrail regression (claim-start volume holds and the unavailable-name path
    stays graceful); otherwise hold.
---

# Claim a Decentraland NAME

## Current capability

The production route reads availability and PRICE from the Ethereum registrar/controller. It verifies the controller's accepted MANA token and registrar, the connected account and the chain before sending. Approval requests only the quoted amount; registration waits for a successful receipt and verifies the token owner before enabling return to World publishing.

Wallet rejection, unavailable RPC, insufficient balance, changed price/account, reverts and pending receipts are distinct failures. Retry resumes the failed step and checks an already-submitted transaction rather than submitting it again, including after a page reload in the same browser tab. Storybook retains explicit simulations. NAMEs cannot be purchased with Credits here.

The local browser proof uses a mock Ethereum wallet, including a taken-name edit, rejected approval, delayed receipt, pending-registration reload, encoded approval/registration calls, mobile layout and return to the originating project. Mainnet configuration was checked read-only; no paid mainnet registration was performed. This does not establish a completed production rollout.

## Assumptions and measurement

A candidate passing local validation is not necessarily available on chain.
Availability, payment approval, transaction submission and confirmed ownership
are distinct. Returning to world publishing must require a real owned NAME.

`mk_claim_name_confirm_rate` measures intent to confirm, not a minted NAME.
The current single-arm draft establishes no comparative improvement; a comparison design and production measurements remain prerequisites for the experiment decision.
Use the shared [Marketplace assumptions](../../../../../../docs/product-capabilities.md#marketplace).
