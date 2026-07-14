---
id: marketplace-transfer
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step transfer wizard (select asset -> enter recipient ->
    review -> confirm -> submit) increases the share of started transfers that
    reach the on-chain confirm step, even with the transfer stubbed, while the
    explicit review + irreversibility warning keep mistaken-recipient errors low.
  because: >-
    Transferring an NFT is irreversible and high-stakes; breaking it into legible
    steps with an address-validation gate and a review screen reduces uncertainty
    and fat-finger mistakes, so more owners who start a transfer push through to
    the wallet confirmation instead of abandoning at an opaque single-shot form.
metric:
  primary: mk_transfer_confirm_rate
  numerator: mk_transfer_confirm_reached
  denominator: mk_transfer_started
  guardrails:
    - mk_transfer_started
    - mk_transfer_invalid_recipient
experiment:
  key: mk_transfer_wizard
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
    Ship if mk_transfer_confirm_rate improves by at least the MDE with no
    guardrail regression (transfer-start volume holds and invalid-recipient
    attempts do not rise); otherwise hold.
---

# Transfer an owned NFT (on-chain transfer stubbed)

The transfer flow (`/marketplace/transfer`) lets a wallet send an owned NFT to
another address. It is a five-step XState wizard:

1. **select-asset** -- pick one of the wallet's owned assets (read live from the
   credits gateway `/credits/v1/users/{address}/wearables`, with the bundled
   fixture as a fallback when the gateway is empty/unreachable).
2. **enter-recipient** -- type/paste the destination address; an address-validity
   gate blocks malformed input and emits `mk_transfer_invalid_recipient`.
3. **review** -- confirm asset + recipient with the irreversibility warning.
4. **confirm-transfer** -- the wallet Web3 secure-confirm screen (`Web3Confirm`).
5. **submit-tx** -> **success** -- the transfer is SUBMITTED and confirmed.

- **Primary metric:** `mk_transfer_confirm_rate` =
  `mk_transfer_confirm_reached` / `mk_transfer_started`.
- **Guardrails:** transfer-start volume (`mk_transfer_started`) and
  invalid-recipient attempts (`mk_transfer_invalid_recipient`) must stay healthy.
- **Events:** `mk_transfer_started` (entering enter-recipient with an asset),
  `mk_transfer_asset_selected` (`{item_id}`), `mk_transfer_recipient_entered`
  (`{recipient}`), `mk_transfer_invalid_recipient`, `mk_transfer_reviewed`,
  `mk_transfer_confirm_reached`, `mk_transfer_submitted` (stub),
  `mk_transfer_completed` (`{tx_hash, stub:true}`).

## Data reality / simulation

There is **no catalyst transfer endpoint** -- ERC-721 `safeTransferFrom` is an
on-chain write that this environment does not perform. The submit + confirmation
are **SIMULATED** by the XState machine's `runTransfer` actor (it resolves a
fabricated, clearly-stubbed `txHash` after a short delay; it never signs or
broadcasts a transaction). The flow, states, telemetry and address validation
are real; only the final commit is a stub. The owned-asset list is real catalyst
data (credits gateway) with a fixture fallback.
