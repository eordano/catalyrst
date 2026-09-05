---
id: marketplace-browse
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A live, filterable collectibles grid (category, rarity, on-sale, sort, search)
    lets visitors find wearables and emotes fast, raising the share of browse
    sessions that open at least one asset detail versus a static catalog.
  because: >-
    Visitors arrive with intent to discover; surfacing real on-sale items with
    rarity and price up front, plus URL-addressable filters, reduces the effort to
    reach a relevant item and lifts browse-to-detail conversion.
metric:
  primary: mk_browse_viewed -> mk_asset_viewed conversion
  numerator: mk_asset_viewed
  guardrails:
    - mk_shop_filter
    - mk_shop_card_clicked
experiment:
  key: marketplace_browse
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.18
  mde: 0.04
decision:
  rule: >-
    Ship the live filterable grid if the browse-to-detail conversion
    (mk_asset_viewed sessions / mk_browse_viewed sessions) beats the static
    baseline with 95% confidence; otherwise iterate on filters.
---

# Marketplace -- Browse collectibles grid with filters

Visitor opens the Collectibles grid (live `/credits/v1/catalog`) and narrows it
with URL-addressable filters: category (wearable / emote / ens), rarity, an
on-sale toggle, sort (newest / cheapest / most expensive), and free-text search.

Journey + metrics:

- `/marketplace` loads the SSR grid -> `mk_browse_viewed { count, total }`.
- Changing a filter/sort via the URL (`?category=`, `?rarity=`, `?onSale=true`,
  `?sortBy=`, `?search=`) refetches on the server ->
  `mk_filter_applied { filter, value }`.
- Clicking an NftCard -> `mk_browse_card_clicked { item_id }` -> navigates to
  `/marketplace/:id`.

The buy flow is DEFERRED (the catalyst `/v1/trades` and `/v1/items` endpoints are
empty stubs), so detail pages surface a graceful "buying coming soon" state.
