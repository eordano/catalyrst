---
id: marketplace-sell-list
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step sell wizard (select asset -> set price -> set expiration
    -> approve -> sign -> confirm) increases the share of started listings that
    reach the sign/confirm step.
  because: >-
    Listing an NFT bundles several unfamiliar decisions (pricing, expiration, an
    NFT approval, then an EIP-712 signature). Splitting them into explicit,
    legible steps with a re-confirm of the price reduces uncertainty and mis-priced
    bail-outs, so more owners who start a listing push through to the signature
    step instead of abandoning at an opaque single-shot sell modal.
metric:
  primary: mk_sell_confirm_rate
  numerator: mk_sell_confirm_reached
  denominator: mk_sell_started
  guardrails:
    - mk_sell_started
    - mk_sell_price_invalid
experiment:
  key: marketplace_sell_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
        requireReconfirm: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if mk_sell_confirm_rate improves by at least the MDE with no guardrail
    regression (listing-start volume holds and the invalid-price rate does not
    rise); otherwise hold.
---

# Sell / list an owned NFT for sale (create order)

The sell wizard (`/marketplace/sell`) breaks listing an owned NFT into explicit
steps: pick one of your owned assets, set a MANA price, choose an expiration
date, approve the NFT for the Marketplace contract, sign the order (EIP-712),
then confirm. This story tracks whether the wizard increases the share of
started listings that reach the sign/confirm step.

The on-chain order placement **fails closed**: the shipped default
`failClosedCreate()` throws `listing unavailable: order relayer not configured`
and the wizard shows its error state, because `routes/marketplace.sell.tsx`
passes no `createOrder`. Flow, states, and metrics are real; the final commit is
unavailable until a `createOrder` implementation is injected by the route.

The real implementation exists behind that default:
`buildCreateOrder({identity, provider, address})`
(`lib/catalyst/marketplace/sell.ts`) reads `isApprovedForAll`, sends
`setApprovalForAll` for the off-chain marketplace contract of the item's chain
and waits for a real receipt, reads the contract/signer signature indexes, signs
the EIP-712 `Trade` with the seller's wallet, and POSTs it to
`/market/v1/trades`. The `approve-nft` and `sign-order` steps are consent gates;
every chain call happens once the wizard reaches `confirm`, so a listing is
either fully signed and stored or it fails with the reason. It stays unrouted
until `catalyrst-market` grows that write route -- the crate is `get`-only today
-- and until a testnet rehearsal has run.

Data reality: owned assets come from `GET /credits/v1/users/{address}/wearables`
(live, but returns `{data:{elements:[]}}` for every probed address), so the
loader falls back to an empty owned-assets list (no fixture fallback exists).

- **Primary metric:** `mk_sell_confirm_rate` = `mk_sell_confirm_reached` / `mk_sell_started`.
- **Guardrails:** listing-start volume (`mk_sell_started`) and the invalid-price
  rate (`mk_sell_price_invalid`) must stay healthy.
- **Events:** `mk_sell_started` (on asset select), `mk_sell_asset_selected`
  (`{item_id}`), `mk_sell_price_set` (`{price_mana}`), `mk_sell_price_invalid`,
  `mk_sell_expiration_set` (`{expires_at}`), `mk_sell_approve_reached`,
  `mk_sell_sign_reached`, `mk_sell_confirm_reached`, `mk_sell_completed`
  (`{order_id, approval_tx_hash, price_mana}` -- `approval_tx_hash` is null when
  the marketplace was already approved), `mk_sell_failed` (`{reason}`).
