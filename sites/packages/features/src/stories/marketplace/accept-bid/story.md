---
id: marketplace-accept-bid
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step accept-bid wizard (review the bid -> connect -> approve
    the NFT -> confirm -> submit) increases the share of started bid-accepts that
    reach the confirm step, even with the on-chain accept stubbed.
  because: >-
    Accepting a bid is a high-stakes, irreversible sale that also requires a
    separate NFT approval most owners do not expect. Making each obligation
    explicit and legible reduces hesitation, so more owners who open a bid push
    through to confirm instead of abandoning at an opaque single-shot accept.
metric:
  primary: mk_accept_bid_confirm_rate
  numerator: mk_accept_bid_confirm_reached
  denominator: mk_accept_bid_started
  guardrails:
    - mk_accept_bid_started
    - mk_accept_bid_rejected
experiment:
  key: mk_accept_bid_wizard
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
    Ship if mk_accept_bid_confirm_rate improves by at least the MDE with no
    guardrail regression (accept-start volume holds and explicit rejections do
    not spike); otherwise hold.
---

# Accept a bid on an owned NFT (on-chain accept stubbed)

The accept-bid wizard (`/marketplace/accept-bid`) walks an owner through
accepting a received bid: **review the bid** (a received-bid row from the
account Bids section) -> **connect wallet** -> **approve the NFT** for the
trade contract -> **confirm** the sale terms -> **submit** the transaction ->
**success**. It tracks whether the explicit step sequence increases the share of
started accepts that reach the confirm step.

- **Primary metric:** `mk_accept_bid_confirm_rate` =
  `mk_accept_bid_confirm_reached` / `mk_accept_bid_started`.
- **Guardrails:** accept-start volume (`mk_accept_bid_started`) and explicit
  rejections (`mk_accept_bid_rejected`) must stay healthy.
- **Events:** `mk_accept_bid_started` (review -> connect), `mk_accept_bid_wallet_connected`,
  `mk_accept_bid_nft_approved`, `mk_accept_bid_confirm_reached` (entering confirm),
  `mk_accept_bid_submitted` (submitting the tx), `mk_accept_bid_completed` (stub),
  `mk_accept_bid_rejected` (owner declines a bid), plus `experiment_exposed`.

Data reality: the live catalyst bids endpoint (`/credits/v1/bids`) returns an
empty result set, so the received-bid is seeded from
`app/fixtures/marketplace-accept-bid.json` (a faithful `@dcl/schemas` NFTBid
shape). The accept POST (`/credits/v1/federation/bid/accept`) and the NFT
approval are **SIMULATED** inside the XState machine -- the flow, states, and
metrics are real; the final on-chain commit is a clearly-noted stub.
