---
id: marketplace-land
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing live LAND parcels and estates with coordinates drives LAND
    exploration even though price and order data are unavailable from catalyst.
  because: >-
    LAND seekers want to see what exists and where; a live grid of real parcels
    and estates with coordinates (and a parcel count for estates) gives a concrete
    sense of the map and invites clicking through, despite the missing price data.
metric:
  primary: mk_land_viewed count and mk_land_card_clicked rate
  guardrails: []
experiment:
  key: marketplace_land
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.0
  mde: 0.05
decision:
  rule: >-
    Keep the LAND grid if mk_land_viewed sustains traffic and mk_land_card_clicked
    shows visitors engage with parcels/estates; otherwise revisit once order/price
    data lands in catalyst.
---

# Marketplace -- Browse LAND (parcels & estates)

Visitor opens `/marketplace/land` (live `/credits/v1/nfts?category=parcel|estate`)
and toggles between parcels and estates via the URL.

Journey + metrics:

- `/marketplace/land` loads the SSR LAND grid ->
  `mk_land_viewed { count, category }`.
- Toggling `?category=parcel` vs `?category=estate` refetches on the server ->
  `mk_land_viewed` re-fires for the new category.
- Cards render coordinates (parcel `x,y`) or the estate parcel count via the
  NftCard `metaRight` slot; price is omitted ("Not for sale") because catalyst
  has no order/rental data for LAND at this gateway (api-coverage-gaps).
- Clicking a LAND card -> `mk_land_card_clicked { nft_id } `-> navigates to
  `/marketplace/:id`.
