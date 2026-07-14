---
id: marketplace-claim-name
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step claim flow (enter name -> check availability -> approve
    MANA -> confirm -> mint) increases the share of started NAME claims that
    reach the on-chain confirm step, even with the mint stubbed.
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

# Claim / mint a Decentraland NAME (ENS)

The Claim-NAME wizard (`/marketplace/claim-name`) breaks minting a DCL NAME into
explicit, URL-addressable steps: enter a name, check availability, approve MANA,
confirm, submit the mint tx, success. Each step makes the cost (100 MANA on
Ethereum Mainnet), the registrar approval, and the irreversible mint legible
before the user commits. This story tracks whether the guided flow increases the
share of started claims that reach the on-chain confirm step.

- **Primary metric:** `mk_claim_name_confirm_rate` =
  `mk_claim_name_confirm_reached` / `mk_claim_name_started`.
- **Guardrails:** claim-start volume (`mk_claim_name_started`) and the
  unavailable-name path (`mk_claim_name_unavailable`) must stay healthy.
- **Events:** `mk_claim_name_started` (entering check-availability, carries the
  candidate name), `mk_claim_name_available` | `mk_claim_name_unavailable`,
  `mk_claim_name_mana_approved`, `mk_claim_name_confirm_reached`,
  `mk_claim_name_submitted` (stub tx), `mk_claim_name_completed` (stub).

## Data reality (real vs simulated)

- **Real:** the existing-names context is LIVE catalyst
  (`/credits/v1/users/{address}/names`), used to seed the "names you already
  own" / taken set. The NAME economics (100 MANA, Ethereum Mainnet, DCLRegistrar
  address, 2..15 alphanumeric validation) are protocol constants pinned from
  `decentraland/marketplace modules/ens/utils.ts`. The flow, states, telemetry,
  and validation are all real.
- **Simulated / deferred:** the DCLRegistrar `available(name)` read and the
  `register` mint are SIMULATED via the XState machine (read-only realm -- no
  on-chain writes). The MANA approval, tx submission, and final commit are
  clearly-noted stubs (`simulated: true` in the emitted props). Availability is
  classified locally against the live owned-name set + a small known-taken list.
