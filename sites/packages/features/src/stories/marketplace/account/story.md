---
id: marketplace-account
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A single account page that surfaces a user's owned assets alongside their
    active store (on-sale, on-rent, collections, bids) -- all URL-addressable via
    ?tab -- lets owners review and manage their portfolio in one place, raising
    the share of account sessions that open a manage/list action versus the
    legacy split screens.
  because: >-
    Owners arrive to check or act on what they hold; co-locating owned items with
    live listing/rental/bid state and deep-linkable tabs cuts the navigation cost
    of finding a specific asset and lifts the account-to-action conversion.
metric:
  primary: mk_account_viewed -> mk_account_tab_changed conversion
  guardrails:
    - mk_account_tab_changed
    - mk_account_asset_clicked
experiment:
  key: marketplace_account
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.32
  mde: 0.05
decision:
  rule: >-
    Ship the unified account page if the tab-engagement rate
    (mk_account_tab_changed sessions / mk_account_viewed sessions) beats the
    legacy split-screen baseline with 95% confidence; otherwise iterate on the
    default tab and section ordering.
---

# Marketplace -- Account page (owned assets, on-sale, on-rent, collections, bids)

The signed-in owner opens their account page (live catalyst:
`/credits/v1/users/{address}/wearables` + `/emotes` + `/names` and
`/credits/v1/orders?owner={address}`). The page composes the ui3
`MkAccountPage` / `MkAccountPage2` / `MkAccountCollectionsSection` /
`MkOnSaleOnRentAccountSections` screens under `AccountChrome`-style chrome, and
every section is URL-addressable via `?tab=`.

Journey + metrics:

- `/marketplace/account` (overview) loads the SSR owned grid ->
  `mk_account_viewed { owned, on_sale, on_rent, names, source }`.
- `?tab=on-sale` / `?tab=on-rent` switch to the OnSale/OnRent store table ->
  `mk_account_tab_changed { tab }`.
- `?tab=collections` renders the CollectionList ->
  `mk_account_tab_changed { tab: "collections" }`.
- `?tab=bids` renders the bids table (catalyst exposes no bids endpoint yet, so
  this tab is a graceful "no bids" state -- DEFERRED, see below) ->
  `mk_account_tab_changed { tab: "bids" }`.
- Clicking an owned NftCard -> `mk_account_asset_clicked { item_id }` ->
  navigates to `/marketplace/:id`.

Data source: live catalyst only -- NO fixtures. The loader reads the wallet's
real owned wearables/emotes/names and open listings from the catalyrst market
backend (`/market/v1/users/:address/{wearables,emotes,names}` and
`/market/v1/orders?owner=:address`), Zod-validates each envelope, and adapts to
the view model. With no `?address=` (no connected wallet server-side) the page
renders an EXPLICIT empty state (`source: "empty"`, `fallback: true`); on a
per-section live-fetch failure that section falls back to an explicit empty list,
never to demo data.

Deferred / simulated:

- `?tab=bids` -- catalyst has no `/credits/v1/bids` endpoint, so the bids tab
  renders a faithful empty state instead of live bid rows.
- On-sale rows are projected from the bare `/orders` feed, which carries no
  asset name/category; the row label is synthesized from the issued id. A
  catalog join (deferred) would supply the real name + rarity.
- The Collections section uses the account-collection placeholder shape (the
  `/credits` gateway exposes no creator-collection list endpoint).
