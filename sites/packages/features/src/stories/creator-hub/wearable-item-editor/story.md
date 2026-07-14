---
id: creator-wearable-item-editor
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Guiding a creator through adding/editing a wearable as four legible,
    URL-addressable steps (model -> category -> rarity -> price) increases the
    share of opened item editors that reach a Save, even with the on-chain mint
    and the builder-server write simulated.
  because: >-
    Rarity and price are the two decisions creators most often stall on -- rarity
    silently caps max supply and price gates the primary sale -- so an opaque
    single-panel editor lets them abandon mid-edit; sequencing model -> category
    -> rarity (with its supply cap shown) -> price (with the cap pre-filling the
    listing) removes that uncertainty, so more creators who open the editor push
    through to Save.
metric:
  primary: bd_item_save_rate
  numerator: bd_item_saved
  guardrails:
    - bd_item_rarity_set
    - bd_item_price_set
experiment:
  key: bd_wearable_item_editor
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
    Ship if bd_item_save_rate (bd_item_saved / experiment_exposed) improves by at
    least the MDE with no guardrail regression (the rarity and price steps both
    stay healthy and are not skipped on the way to Save); otherwise hold.
---

# Add/edit a wearable in the item editor (model -> category -> rarity -> price)

The wearable item editor (`/create/wearables/item-editor`) walks a creator
through the four decisions that define a sellable wearable as explicit,
URL-addressable steps over the ui3 `BdItemEditor` surface:

1. **model** -- pick the `.glb` representation (upload simulated).
2. **category** -- choose the `WearableCategory` (schemas) the item occupies.
3. **rarity** -- pick the `Rarity` (schemas); the step shows the hard **max
   supply** that rarity implies (unique 1 ... common 100000).
4. **price** -- set the primary-sale MANA price (or mark it free); the rarity's
   max supply pre-fills the listing context. Then **save**.

This story tracks whether the stepped editor increases the share of opened
editors that reach a Save.

- **Primary metric:** `bd_item_save_rate` = `bd_item_saved` / `experiment_exposed`.
- **Guardrails:** `bd_item_rarity_set` and `bd_item_price_set` must stay healthy --
  Save should not be reached with rarity or price skipped.
- **Events:** `experiment_exposed` (loader, once the wizard surface renders),
  `bd_item_rarity_set` (`{ item, rarity, max_supply }`) on committing the rarity,
  `bd_item_price_set` (`{ item, price, free }`) on committing the price,
  `bd_item_saved` (`{ item, rarity, price }`, `stub: true`) on the simulated
  persist. **Revert** discards unsaved edits and never crashes.

Data reality: there is NO fixture. The collection picker is read live from
catalyrst-builder (`GET /v1/{address}/collections`, Zod-validated) at SSR; the
per-collection item list is auth-gated (`GET /v1/collections/{id}/items` -> 401 at
SSR) so it is hydrated client-side via the signed-fetch session once a wallet is
connected (mirrors `builder.collections_.$id`'s `useLiveItems`). Categories,
rarities, and max-supply are static domain constants mirroring
`decentraland/schemas/src/platform/item` (WearableCategory, BodyShape) and
`decentraland/schemas/src/dapps/rarity.ts` (Rarity + maxSupply); item `price` /
`beneficiary` / `rarity` follow
`decentraland/builder-server/src/Item/Item.types.ts`. A brand-new-item draft
needs no wallet. The builder-server item `PUT /v1/items/:id`, the S3 `.glb` upload,
and the on-chain mint/list that a real price implies are auth-gated and
unreachable from this realm, so the **model upload, the price listing, and the
final Save are SIMULATED** in the XState machine (the flow, states, and metrics
are real; the commit is a clearly-noted stub). Noted as deferred.
