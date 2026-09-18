---
id: marketplace-buy-nft
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, legible buy wizard (review -> connect -> approve MANA -> confirm ->
    sign -> success) increases the share of started NFT purchases that reach the
    signed-commit step, versus an opaque single-shot buy modal.
  because: >-
    Secondary-market buys require several wallet interactions (token allowance,
    then an EIP-712 trade signature). Surfacing each as an explicit, recoverable
    step reduces uncertainty about what the wallet is about to do, so more buyers
    who start a purchase push through to confirm instead of bailing at an
    ambiguous approval prompt.
metric:
  primary: mk_buy_confirm_rate
  numerator: mk_buy_confirm_reached
  denominator: mk_buy_started
  guardrails:
    - mk_buy_started
    - mk_buy_mana_approved
    - mk_buy_failed
experiment:
  key: mk_buy_wizard
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
    Ship if mk_buy_confirm_rate improves by at least the MDE with no guardrail
    regression (buy-start volume holds, the MANA-approval step does not become a
    new drop-off cliff, and mk_buy_failed does not rise); otherwise hold.
---

# Buy a listed NFT

The production route reads a real listing through `loadBuyListing`. Missing and
unavailable listings have separate states; it does not invent a fallback price.

## Current capability

Secondary-market purchase submission is **unavailable**. The route injects
`unavailablePurchase` for connect, approval and commit. It requests no wallet
signature or token approval and cannot produce a successful purchase receipt.
Production also disables URL step previews, so `?step=success` cannot masquerade
as a completed purchase. Storybook may still explicitly exercise simulated steps.
Collection-item checkout is a separate flow; its readiness does not imply this
secondary-market route can settle an order.

## Assumptions and measurement

A live listing, connected wallet, correct chain, sufficient allowance and a
working transaction submission path are separate prerequisites. A typed-data
signature is not a transaction or purchase. Completion requires an authoritative
result for the buyer and asset.

`mk_buy_confirm_rate` measures reaching confirmation, not ownership transfer.
This single-arm draft cannot demonstrate lift over a control. Enable a real
writer and define a comparison before applying the decision rule. Use the
shared [Marketplace assumptions](../../../../../../docs/product-capabilities.md#marketplace).
