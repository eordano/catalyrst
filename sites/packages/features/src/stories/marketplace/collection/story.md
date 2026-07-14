---
id: marketplace-collection
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A live collection-detail page that surfaces every item in a creator
    collection with its rarity, stock, and price up front -- plus the collection
    floor and creator -- raises the share of collection views that open at least
    one item detail versus sending visitors back to the flat catalog.
  because: >-
    Visitors who reach a collection arrive with creator/brand intent; showing the
    full item table with floor and creator context, and letting them re-sort by
    price (?sortBy=), reduces the effort to find the item they want and lifts
    collection-to-detail conversion.
metric:
  primary: mk_collection_viewed -> mk_collection_item_clicked conversion
  guardrails:
    - mk_collection_sorted
    - mk_collection_empty
experiment:
  key: marketplace_collection
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.22
  mde: 0.04
decision:
  rule: >-
    Ship the live collection-detail page if the collection-to-detail conversion
    (mk_collection_item_clicked sessions / mk_collection_viewed sessions) beats
    the catalog-only baseline with 95% confidence and the empty-collection path
    (mk_collection_empty) stays graceful; otherwise iterate on the layout.
---

# Marketplace -- Collection detail page (items + floor + creator)

Visitor opens a single creator collection (live `/credits/v1/collections` +
`/credits/v1/items?contractAddress=`) and browses every item in it: name,
category, rarity, stock, and price, rendered in the ui3 `MkCollectionPage` table.
The header shows the collection name, an On Sale badge, the floor price, and the
creator address. The item table is re-sortable via the URL (`?sortBy=`), which
refetches the items on the server.

Journey + metrics:

- `/marketplace/collection?contract=<addr>` loads the SSR collection page ->
  `mk_collection_viewed { contract, item_count, floor }` (or
  `mk_collection_empty { contract }` when the collection / items are empty).
- Re-sorting via `?sortBy=cheapest|most_expensive|recently_listed|recently_sold`
  refetches the items on the server -> `mk_collection_sorted { sort_by }`.
- Clicking an item row -> `mk_collection_item_clicked { item_id }` -> navigates
  to `/marketplace/:id`.

Data reality: the collection meta and its priced items are live from
catalyrst-market; when the gateway is empty/unreachable the loader falls back to
`app/fixtures/marketplace-collection.json` (a captured live collection) so the
page always renders. There is NO buy action on this page -- purchasing is the
marketplace-buy story (DEFERRED while `/v1/trades` is a stub); the row click
hands off to the asset-detail page.
