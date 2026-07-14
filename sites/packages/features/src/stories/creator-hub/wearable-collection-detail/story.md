---
id: creator-wearable-collection-detail
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Showing a wearables collection's real items on a single Creator Hub
    /create/wearables/collections/{id} page -- split into Wearables / Emotes with
    each item's rarity, category, price, supply and per-row status -- lets a
    creator scan their drop and open the exact item they came to edit in one
    click, raising the share of collection views that convert to an item open.
  because: >-
    When the collection detail surfaces every item with a recognisable name,
    rarity badge, price and per-row status pill, a creator can immediately spot
    the item that needs work (e.g. "not ready", missing price) and open its
    editor directly -- instead of paging through a flat list -- so a higher share
    of collection views convert to an item-editor open.
metric:
  primary: creator_collection_item_click_rate
  numerator: creator_collection_item_clicked
  denominator: creator_collection_detail_viewed
  guardrails:
    - creator_collection_detail_viewed
    - creator_collection_tab_changed
experiment:
  key: creator_wearable_collection_detail
  unit: session
  variants:
    - id: split-detail
      weight: 1
      flags:
        splitItems: true
  baseline: 0.5
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if creator_collection_item_click_rate improves by at least the MDE with
    no regression in creator_collection_detail_viewed volume (and the
    Wearables/Emotes tab toggle keeps working); otherwise hold.
---

# Collection detail: items split Wearables/Emotes with rarity, category, price, status

The single-collection page of the Creator Hub lists one wearables collection's
items split into **Wearables** and **Emotes**, each row carrying its rarity
badge, category, price (and supply once published) and a per-row status pill,
inside ui3's `BdCollectionDetail`. This story (persona: CREATOR) tracks whether a
scannable, clickable item table lifts the open-through rate from a collection
view to an item's editor.

Single shipping variant (`split-detail`).

- **Primary metric:** `creator_collection_item_click_rate` =
  `creator_collection_item_clicked` / `creator_collection_detail_viewed`
  (collection -> item-editor open-through).
- **Guardrails:** `creator_collection_detail_viewed` volume and
  `creator_collection_tab_changed` must not regress.
- **Events:**
  - `creator_collection_detail_viewed` on load
    (`{ id, tab, itemCount, source }`),
  - `creator_collection_tab_changed` on the Wearables/Emotes toggle (`{ tab }`)
    -- only meaningful when the collection has BOTH item types,
  - `creator_collection_item_clicked` on an item-row click
    (`{ id, itemId, tab, kind }`).

Journey steps (URL-addressable):

1. Open `/create/wearables/collections/{id}` --
   `creator_collection_detail_viewed { id, tab: "wearables", itemCount }`.
2. Switch to the Emotes tab
   `/create/wearables/collections/{id}?tab=emotes` (the Wearables/Emotes tabs
   render only when the collection has both types) --
   `creator_collection_tab_changed { tab: "emotes" }`.
3. Click an item row -> opens the item editor
   `/create/wearables/item-editor?collection={id}&item={itemId}` --
   `creator_collection_item_clicked { id, itemId, tab, kind }`.
4. Empty / unknown collection deep links are addressable too: an empty
   collection (`/create/wearables/collections/{empty-id}`) renders the
   "Looking good!" sparkles empty state (no tabs, no table) -- no demo/fixture
   collection is substituted.
5. **Publish Collection** (header) -> `/create/wearables/publish?collection={id}`;
   **Add Items** -> `/create/wearables/item-editor?collection={id}`; the back
   chevron -> `/create/wearables`.

Data source: live only. The item shapes derive from `decentraland/builder-server`
`CollectionAttributes` / `ItemAttributes` (`FullItem`) + `decentraland/schemas`
`Rarity` / `WearableCategory` / `EmoteCategory`. The SSR loader calls
catalyrst-builder `GET /v1/collections/{id}/items` (public read); when a wallet
is connected, the client re-reads items + collection meta through the
signed-fetch session (`useLiveItems`) and overlays them. There is **no fixture
fallback** -- an unreadable collection renders the empty state and the route
reports `source` ("catalyst" | "empty") on the view event.

Deferred / SIMULATED: the item-row click opens the existing
`/create/wearables/item-editor` route (item-editor flow owned by another story),
keeping the journey URL-addressable. Row-menu actions with no in-app equivalent
yet (See in Decentraland, Move to another collection, Reset changes, Delete
item, Mint Items) render disabled with a "Not available on this realm yet"
title instead of dead-clicking. The collection HEADER (name / publish state /
on-sale) comes from the collection-meta read when signed in, else a neutral
placeholder header.
