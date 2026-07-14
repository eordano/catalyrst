---
id: marketplace-lists
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Letting visitors save items into named lists (and a default Favorites
    collection) and revisit them later raises the share of return sessions that
    re-open a saved item, versus a marketplace with no save affordance.
  because: >-
    Discovery and purchase intent rarely happen in one session; a durable place
    to park interesting wearables and emotes gives visitors a reason to return
    and a fast path back to the exact items they cared about, lifting
    list-to-detail re-engagement.
metric:
  primary: mk_list_opened -> mk_list_item_clicked conversion
  guardrails:
    - mk_lists_viewed
    - mk_list_opened
decision:
  rule: >-
    Ship saved lists if the list-to-detail conversion (mk_list_item_clicked
    sessions / mk_list_opened sessions) clears the baseline with 95% confidence
    and mk_lists_viewed sustains; otherwise iterate on list presentation before
    wiring a live list/pick service.
experiment:
  key: marketplace_lists
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.22
  mde: 0.05
---

# Marketplace -- Lists & favorites (saved-item collections)

Visitor opens their saved-item collections (a default **My Favorites** list plus
any named lists they created), then opens one to browse the items inside and
click through to an asset detail.

## Data source (SIMULATED lists, LIVE items)

The Catalyst gateway does **not** mount a list/pick service: both
`/credits/v1/lists` and `/lists/v1/lists` return **404** (verified live). So:

- **List metadata is SIMULATED** -- a local fixture
  (`app/fixtures/marketplace-lists.json`) whose List/Pick shapes are derived from
  decentraland/marketplace's favorites-component
  (`webapp/src/modules/favorites/types.ts` -- `List`; and
  `vendor/decentraland/favorites/types.ts` -- `BaseList` / `ListOfLists`).
- **Item rows are LIVE** -- seeded from the real `/credits/v1/catalog?isOnSale=true`
  endpoint, so names, thumbnails, rarities, networks and MANA prices are real.
- **Create / rename / delete a list, and pick/unpick an item, are DEFERRED** --
  there is no write API at this gateway. The page is read-only browse + detail.

## Journey + metrics

- `/marketplace/lists` loads the SSR lists overview ->
  `mk_lists_viewed { count }`.
- Opening a list (clicking a list card, or deep-linking `?listId=<id>`) renders
  the items inside that list -> `mk_list_opened { list_id, items_count }`.
- Clicking a saved item's NftCard -> `mk_list_item_clicked { list_id, item_id }`
  -> navigates to `/marketplace/:id`.
- `?listId=<id>` makes the open-list view URL-addressable (deep-linkable and
  screenshot-able); clearing it returns to the overview.
