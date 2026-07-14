---
id: marketplace-rent-land
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step rental wizard (review LAND -> pick a rental period ->
    confirm the day-rate -> approve MANA -> sign -> confirm) increases the share
    of started rentals that reach the signing step, versus a single opaque
    "rent" action, even with the on-chain commit simulated.
  because: >-
    Renting LAND couples two unfamiliar decisions (which period tier, how many
    days) with two wallet actions (approve MANA, sign the rental). Splitting them
    into explicit, legible steps with a running MANA quote reduces uncertainty
    about cost and what each wallet prompt does, so more renters who start push
    through to signing instead of bailing at an ambiguous all-in-one prompt.
metric:
  primary: mk_rent_sign_rate
  numerator: mk_rent_sign_reached
  denominator: mk_rent_started
  guardrails:
    - mk_rent_started
    - mk_rent_abandoned
experiment:
  key: marketplace_rent_wizard
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
    Ship if mk_rent_sign_rate improves by at least the MDE with no guardrail
    regression (rental-start volume holds and abandonment does not rise);
    otherwise hold and iterate on the period/quote step.
---

# Rent a LAND parcel or estate (rentals listing)

The rental wizard (`/marketplace/rent-land`) breaks renting LAND into explicit,
URL-addressable steps: review the LAND, select a rental period tier, set the
number of days (which produces a running MANA quote) or accept the lessor's
offer, approve MANA spending, sign the rental, then confirm. This story tracks
whether the wizard increases the share of started rentals that reach the signing
step.

- **Primary metric:** `mk_rent_sign_rate` = `mk_rent_sign_reached` / `mk_rent_started`.
- **Guardrails:** rental-start volume (`mk_rent_started`) and abandonment
  (`mk_rent_abandoned`, emitted when the user backs out of the flow) must stay
  healthy.
- **Events:** `mk_rent_started` (entering review), `mk_rent_period_selected`
  (`{period_index, min_days, max_days}`), `mk_rent_price_set`
  (`{days, total_mana}`), `mk_rent_mana_approved`, `mk_rent_sign_reached`,
  `mk_rent_signed`, `mk_rent_completed` (stub), `mk_rent_abandoned`,
  `mk_rent_failed` + `mk_rent_retried` on the error path.

Data reality: the `/credits` gateway returns the **live** LAND grid
(`/credits/v1/nfts?category=parcel|estate`) but every row's `rental` field is
`null` -- there is **no rentals read endpoint** at the gateway. So the rental
listings are derived faithfully from the upstream
`decentraland/schemas` rentals contract (`RentalListing` /
`RentalListingPeriod`) and stored in `app/fixtures/marketplace-rent-land.json`
(with live LAND nft metadata). The on-chain rental commit (approve MANA + sign
the rental signature + submit to the Rentals contract) is **SIMULATED** in the
XState machine -- the flow, states, and metrics are real; only the final commit
is a clearly-noted stub.
