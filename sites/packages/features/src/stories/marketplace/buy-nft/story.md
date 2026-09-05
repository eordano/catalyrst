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

# Buy a listed NFT (secondary-market order)

The buy wizard (`/marketplace/buy?item=<id>`) walks a buyer through purchasing a
live secondary-market listing: **review** the listing, **connect-wallet**,
**approve-mana** (ERC-20 allowance), **confirm-purchase**, **submit-tx** (the
EIP-712 signed trade commit), and **success**.

Listing data is LIVE from Catalyst `GET /credits/v1/orders` (cheapest open order
for the item, joined with the `/credits/v1/catalog` row for name/rarity/image),
with `app/fixtures/marketplace-buy-nft.json` as the fallback.

- **Primary metric:** `mk_buy_confirm_rate` = `mk_buy_confirm_reached` / `mk_buy_started`.
- **Guardrails:** buy-start volume (`mk_buy_started`), the approval step
  (`mk_buy_approve_reached`) must not become a new drop-off, and failures
  (`mk_buy_failed`) must not rise.
- **Events (per-transition):** `mk_buy_started` (leave review), `mk_buy_wallet_connected`,
  `mk_buy_mana_approved`, `mk_buy_confirm_reached` (enter submit-tx / sign),
  `mk_buy_completed` (stub), `mk_buy_failed` (sign/commit error), plus
  `experiment_exposed` + `mk_buy_viewed` on the route loader.

## Simulated / deferred

The final on-chain commit is **simulated**. The live endpoint
`POST /credits/v1/federation/trade` requires an EIP-712 / dcl auth-chain signed
payload from a connected wallet and rejects unsigned requests with
`{"ok":false,"message":"auth chain: Invalid Auth Chain"}`. The wizard's
`simulateTradeCommit` actor never signs and never POSTs -- it resolves a stubbed
`{txHash}` after a short delay so the submit/success screens advance. The flow,
states, telemetry, and the live listing/price are all real; only the signed
commit is a clearly-noted stub.
