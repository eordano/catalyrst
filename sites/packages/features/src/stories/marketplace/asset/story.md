---
id: marketplace-asset
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A detail page with image, rarity, collection and a prominent MANA price gives
    enough confidence that buy-intent (Buy clicks) rises, even with the buy flow
    deferred. Making the price more prominent (treatment) lifts buy-intent over a
    standard layout (control).
  because: >-
    Price is the decisive signal for a collectible purchase; surfacing it
    prominently near the primary action reduces hesitation and raises the rate of
    Buy clicks per asset view.
metric:
  primary: mk_buy_clicked rate per mk_asset_viewed
  guardrails:
    - mk_asset_viewed
experiment:
  key: marketplace_price_prominence
  unit: session
  variants:
    - id: control
      weight: 50
      flags:
        prominentPrice: false
    - id: treatment
      weight: 50
      flags:
        prominentPrice: true
  baseline: 0.12
  mde: 0.04
decision:
  rule: >-
    Ship treatment if mk_buy_clicked / mk_asset_viewed is higher than control with
    95% confidence and mk_asset_viewed does not regress; otherwise keep control.
---

# Marketplace -- View a collectible asset detail

Visitor lands on `/marketplace/:id` (id = `<contractAddress>-<itemId>`). The
loader fetches the single catalog item (by `contractAddress` + `itemId`), resolves
the `marketplace_price_prominence` variant, emits the exposure event, and renders
the ui3 MkAssetPage with image, rarity badge, collection and MANA price.

Journey + metrics:

- Load `/marketplace/:id` -> `experiment_exposed` (trackExposure) +
  `mk_asset_viewed { item_id, rarity, on_sale }`.
- Click Buy -> `mk_buy_clicked { item_id }`; the buy flow is DEFERRED (empty
  `/v1/trades` stub) so a graceful "buying coming soon" state is shown.

A/B: `prominentPrice` (control=false / treatment=true) controls a price-prominence
treatment near the primary action. `?variant=marketplace_price_prominence:<id>` is
honored as a PREVIEW-only QA override.
