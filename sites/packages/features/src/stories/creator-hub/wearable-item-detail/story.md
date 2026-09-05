---
id: creator-wearable-item-detail
status: spec
owner: owner@example.com
hypothesis:
  statement: >-
    Showing a wearable/emote's full read-only detail -- the model preview wash,
    category, rarity & supply, representations or smart-wearable permissions, and
    selling info -- on a single Creator Hub item page lets a creator confirm an
    item is correct and open it in the editor in one click.
  because: >-
    When the detail page surfaces the real shape of an item (the Properties
    metrics box, the Details id/category/supply/URN rows, and either the
    body-shape Representations or the required-permission Permissions card), a
    creator can verify the item at a glance and click Edit to fix or finish it --
    instead of re-opening the editor blind -- so a higher share of detail views
    convert to an editor open.
metric:
  primary: creator_item_edit_clicked
  guardrails:
    - creator_item_detail_viewed
experiment:
  key: creator-wearable-item-detail
  unit: session
  baseline: 0.4
  mde: 0.05
  min_sample: 3000
  variants:
    - id: read-only-detail
      weight: 1
      flags:
        liveItem: true
decision:
  rule: >-
    Ship if the edit-click-through rate (creator_item_edit_clicked /
    creator_item_detail_viewed) improves by at least the MDE with no regression
    in creator_item_detail_viewed volume; otherwise hold.
---

# Wearable item detail (read-only spec): model preview, category, rarity, representations

The CREATOR-persona item detail page renders a single wearable or emote inside
ui3's `BdItemDetail` (which embeds `BuilderChrome`): a left panel with the rarity
model-preview wash, the Properties metrics box and the Details box (id / category
/ supply / collection / URN), and a right cards column with the title +
description/utility + tags, the Selling card (price + beneficiary), and EITHER the
Representations card (plain wearable, body-shape `.glb` rows) OR the Permissions
card (smart wearable, required-permission pills).

Data is sourced live: `loadCreatorItem` resolves the `{contract}-{itemId}` id
against the marketplace catalog (`fetchCatalogItem`) and adapts the row onto the
upstream Item shapes (`decentraland/schemas/src/platform/item`
Wearable/Emote/Metrics/Rarity + `decentraland/builder-server/src/Item`
ItemAttributes / SmartWearableData). There is **no fixture fallback** -- an id
the catalog can't resolve renders the NotFound state (the old
`creator-wearable-item-detail.json` fixture was deleted in the dead-code
sweep).

Single shipping variant (`read-only-detail`). This is a read-only browse/detail
page (loader + components), per spec -- no A/B, no XState.

- **Primary metric:** edit click-through = `creator_item_edit_clicked` /
  `creator_item_detail_viewed` (detail view -> editor open-through).
- **Guardrail:** `creator_item_detail_viewed` volume must not regress.
- **Events:** `creator_item_detail_viewed` on load (`{ id, type }`),
  `creator_item_edit_clicked` on the Edit / Preview-in-Editor action (`{ id }`) --
  which routes to `/create/wearables/item-editor?item={id}`.

Journey steps (URL-addressable):

1. Open `/create/wearables/items/{contract}-{itemId}` (a marketplace catalog
   item id, e.g. an on-sale wearable's `0x...-0`) -- `creator_item_detail_viewed
   { id, type }`.
2. View the Properties / Details / Representations (wearable) or Permissions
   (smart wearable) cards -- rendered from the resolved item.
3. Click **Edit** / **Preview in Editor** -> navigates to
   `/create/wearables/item-editor?item={id}` (the Representations card's Edit
   deep-links to `?step=model`) -- `creator_item_edit_clicked { id }`.
4. Open a missing item id (e.g. `/create/wearables/items/nope`) -> graceful
   NotFound state inside the chrome (no crash).

Deferred / simulated: item imagery is remote in the real product, so ui3 renders
a flat rarity/hue wash as the model preview. The `/create/wearables/item-editor`
destination is owned by the creator-wearable-item-editor story; the Edit CTA fires
`creator_item_edit_clicked` and navigates to that URL-addressable path. Actions
with no in-app write path (Edit Thumbnail, Delete, Move to another collection)
render disabled with a "Not available on this realm yet" title; Copy URN is a
real clipboard copy. On-chain mint / listing is out of scope for this read-only
detail spec.
