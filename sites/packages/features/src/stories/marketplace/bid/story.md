---
id: marketplace-bid
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step bid wizard (set amount -> set expiration -> approve MANA
    -> sign -> confirm) increases the share of started bids that reach the sign
    step, even with the on-chain bid placement stubbed.
  because: >-
    Bidders abandon when the price, expiration, the MANA allowance, and the
    signature all collapse into one opaque modal. Breaking the offer into explicit,
    legible steps reduces uncertainty about what is being authorized, so more
    bidders who set an amount push through to signing instead of bailing.
metric:
  primary: mk_bid_sign_rate
  numerator: mk_bid_sign_reached
  denominator: mk_bid_started
  guardrails:
    - mk_bid_started
    - mk_bid_insufficient_mana
experiment:
  key: marketplace_bid_wizard
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
    Ship if mk_bid_sign_rate improves by at least the MDE with no guardrail
    regression (bid-start volume holds and the insufficient-MANA rate does not
    rise); otherwise hold.
---

# Place a bid on an NFT

The bid flow (`/marketplace/bid?id=<contract>-<itemId>`) breaks placing an offer
into explicit steps: review the asset, set the bid amount, pick an expiration,
approve the MANA allowance, sign the EIP-712 BidPlace order, confirm, and land on
success. This story tracks whether the wizard increases the share of started bids
that reach the sign step.

- **Primary metric:** `mk_bid_sign_rate` = `mk_bid_sign_reached` / `mk_bid_started`.
- **Guardrails:** bid-start volume (`mk_bid_started`) and the insufficient-MANA
  rate (`mk_bid_insufficient_mana`) must stay healthy.
- **Events:** `mk_bid_started` (entering set-amount), `mk_bid_amount_set`
  (`{price}`), `mk_bid_expiration_set` (`{expiration}`), `mk_bid_mana_approved`,
  `mk_bid_sign_reached`, `mk_bid_signed`, `mk_bid_confirmed`, `mk_bid_completed`
  (stub), `mk_bid_insufficient_mana`, `mk_bid_failed`.

Data reality: the asset is a real listed catalog item
(`catalyst:/credits/v1/catalog`). The catalyst `/credits/v1/bids` read endpoint
returns `{results:[],total:0}` (empty), so the open-bids list is faithfully empty
and PLACE is the live path. The bid placement itself -- the EIP-712 signature and
the `POST /credits/v1/federation/bid` (BidPlace) commit -- is SIMULATED in the
XState machine (staging is read-only; we never sign or commit on-chain). The
flow, states, telemetry, and the empty/insufficient-MANA branches are real; only
the final on-chain commit is a clearly-noted stub.
