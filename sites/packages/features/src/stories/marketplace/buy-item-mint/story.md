---
id: marketplace-buy-item-mint
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A clear multi-step mint wizard (review -> connect wallet -> approve MANA ->
    confirm -> submit) increases the share of started mints that reach the
    confirm step, even with the on-chain commit simulated.
  because: >-
    Primary-market minting hides several distinct approvals (wallet connect, MANA
    spend allowance, the mint tx) behind one opaque button; making each step
    explicit and legible reduces uncertainty about what is being signed, so more
    buyers who start a mint push through to the confirm step instead of bailing.
metric:
  primary: mk_mint_confirm_rate
  numerator: mk_mint_confirm_reached
  denominator: mk_mint_started
  guardrails:
    - mk_mint_started
    - mk_mint_failed
experiment:
  key: mk_mint_wizard
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
    Ship if mk_mint_confirm_rate improves by at least the MDE with no guardrail
    regression (mint-start volume holds and the failure path stays graceful and
    retryable); otherwise hold.
---

# Marketplace -- Buy/mint a primary-market item

Buyer opens `/marketplace/mint` (optionally `?item=<contractAddress>-<itemId>`)
to mint a fixed-price primary-market item from a collection. The loader fetches
live priced mint items from `catalyst:/credits/v1/items` (falling back to the
`marketplace-buy-item-mint.json` fixture when the gateway is empty/unreachable),
resolves the single-variant `mk_mint_wizard` experiment, emits the exposure
event, and renders the interactive `<BuyMintWizard>` (XState).

Flow (states map 1:1 to ui3 screens, each URL-addressable via `?step`):

1. **review** (`MkAssetPage` + `AssetActionLayout`) -- confirm item, price, supply.
2. **connect-wallet** (`AssetActionLayout`) -- connect a wallet (simulated).
3. **approve-mana** (`AssetActionLayout`) -- approve the MANA spend allowance
   (simulated ERC-20 approval).
4. **confirm-mint** (`MkBuyFlow`) -- the "Confirm Your Purchase" modal: cost,
   fee, total.
5. **submit-tx** (`MkBuyStatusPage`) -- the in-flight mint transaction (simulated
   commit; the real `POST /credits/v1/federation/trade` needs a wallet
   auth-chain signature -> 401 without one).
6. **success** (`MkSuccessPage`) -- minted; jump in / view item.

Journey + metrics (per-transition `track()`):

- Load -> `experiment_exposed` (trackExposure).
- Leave review -> `mk_mint_started` + `mk_mint_review_confirmed { item_id, price_mana }`.
- Wallet connected -> `mk_mint_wallet_connected`.
- MANA approved -> `mk_mint_mana_approved`.
- Enter confirm -> `mk_mint_confirm_reached` (the primary-metric numerator).
- Submit -> `mk_mint_submitted`; on simulated success -> `mk_mint_completed { stub: true }`.
- On simulated failure -> `mk_mint_failed { step }` (retryable).

- **Primary metric:** `mk_mint_confirm_rate` = `mk_mint_confirm_reached` / `mk_mint_started`.
- **Guardrails:** mint-start volume (`mk_mint_started`) and the failure path
  (`mk_mint_failed`) must stay healthy.

Deep links: `?step=review|connect|approve|confirm|submit|success` hydrate the
wizard AT that step via an XState snapshot (no event replay -> no telemetry
double-fire, the simulated submit actor does not auto-race forward).
`?variant=mk_mint_wizard:wizard` is honored as a PREVIEW-only QA override.

## Simulated / deferred

The **on-chain commit is simulated**. The live data (mint items, their real
`tradeId` / `tradeContractAddress` / price) is real; the wallet connect, the
MANA approval, and the final mint `POST /credits/v1/federation/trade` are
simulated actors in the XState machine (the trade POST requires a wallet
auth-chain signature this surface cannot produce -- it returns 401 "Invalid Auth
Chain"). The flow, the states, and all metrics are real.
