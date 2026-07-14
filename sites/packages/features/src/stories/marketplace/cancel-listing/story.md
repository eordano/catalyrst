---
id: marketplace-cancel-listing
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A clear multi-step cancel-listing wizard (review the active listing ->
    connect wallet -> confirm -> submit) increases the share of started
    cancellations that reach the confirm step, even with the on-chain cancel
    simulated.
  because: >-
    Removing a sale listing is an irreversible-feeling on-chain action; breaking
    it into explicit, legible steps (what is being removed, for how much, and a
    single signer-only confirm) reduces hesitation, so more sellers who start a
    cancel push through to the confirm step instead of bailing at an opaque
    one-shot button.
metric:
  primary: mk_cancel_confirm_rate
  numerator: mk_cancel_confirm_reached
  denominator: mk_cancel_started
  guardrails:
    - mk_cancel_started
    - mk_cancel_not_owner
experiment:
  key: marketplace_cancel_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.5
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if mk_cancel_confirm_rate improves by at least the MDE with no
    guardrail regression (cancel-start volume holds and the not-owner /
    not-for-sale guard paths stay graceful); otherwise hold.
---

# Cancel an active sale listing (cancel stubbed)

The cancel-listing wizard (`/marketplace/cancel?order=<id>`) removes an active
sale listing for an owned NFT. It walks these explicit steps:

1. **review-listing** -- show the active Order (asset, price, expiry) pulled from
   `catalyst:/credits/v1/orders` (owner's `status=open` rows). Guards the
   not-owner / not-for-sale cases.
2. **connect-wallet** -- require a connected signer (the cancel is signer-only).
3. **confirm-cancel** -- the explicit "Remove listing" confirmation.
4. **submit-tx** -- submit the cancel; SIMULATED (see below).
5. **success** -- the listing is removed.

- **Primary metric:** `mk_cancel_confirm_rate` =
  `mk_cancel_confirm_reached` / `mk_cancel_started`.
- **Guardrails:** cancel-start volume (`mk_cancel_started`) and the not-owner
  guard (`mk_cancel_not_owner`) must stay healthy.
- **Events:** `mk_cancel_started` (on the first real step transition),
  `mk_cancel_wallet_connected`, `mk_cancel_confirm_reached`,
  `mk_cancel_submitted`, `mk_cancel_completed` (stub), plus the guard event
  `mk_cancel_not_owner` and the failure/retry events `mk_cancel_failed`.

## Data reality / simulated

Listings data is LIVE from `catalyst:/credits/v1/orders` (the seller's open
orders), with a fixture fallback (`app/fixtures/marketplace-cancel-listing.json`)
when the query is empty/unreachable.

The on-chain cancel is **simulated / deferred**: `POST
/credits/v1/federation/order/cancel` requires a signed auth-chain
(`require_signer`) and enforces *"only the order signer may cancel this order"*
(`catalyrst-market/src/handlers/federation.rs::cancel_order`) -- an unsigned POST
returns 401. We therefore never POST the cancel; the wizard runs a simulated
actor (no network) that builds the `OrderCancel { order_signature_hash, signed_at }`
message shape and resolves the success state. The flow / states / metrics are
real; only the final commit is a clearly-noted stub.
