---
id: marketplace-settings
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Co-locating the marketplace account settings (wallet + on-chain contract
    authorizations) with the storefront editor (cover, description, social
    links) on one URL-addressable page -- switchable via ?tab -- lets sellers go
    from "review my permissions" to "edit my store" without losing context,
    raising the share of settings sessions that open the store-settings editor.
  because: >-
    Sellers who open settings are usually preparing to list or to polish their
    storefront; surfacing the authorization state and the store editor side by
    side, each deep-linkable, removes the navigation cost of hunting for the
    store-settings screen and lifts the settings-to-store-edit rate.
metric:
  primary: mk_settings_viewed -> mk_settings_tab_changed conversion
  guardrails:
    - mk_settings_tab_changed
    - mk_store_field_edited
    - mk_store_save_clicked
experiment:
  key: marketplace_settings
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.28
  mde: 0.05
decision:
  rule: >-
    Ship the unified settings + store editor page if the tab-engagement rate
    (mk_settings_tab_changed sessions / mk_settings_viewed sessions) beats the
    legacy split-screen baseline with 95% confidence; otherwise iterate on the
    default tab and the store-editor field ordering.
---

# Marketplace -- Settings + store-settings editor

The signed-in seller opens their marketplace settings. The page composes the
ui3 `MkSettingsPage` (wallet address + on-chain contract authorizations) and
`MkStoreSettingsEditor` (cover / description / website / social handles) under
the shared `MarketplaceChrome`. The two surfaces are URL-addressable via `?tab=`
so every journey step can be deep-linked and screenshotted:

- `load-settings` -> `/marketplace/settings` (the authorizations view) ->
  `mk_settings_viewed { granted, source }`.
- `edit-store` -> `/marketplace/settings?tab=store` (the store editor) ->
  `mk_settings_tab_changed { tab: "store" }`.
- `?tab=` (`authorizations` | `store`) switches the active surface ->
  `mk_settings_tab_changed { tab }`. Editing any store field emits
  `mk_store_field_edited { field }`; clicking Save emits `mk_store_save_clicked`.

## Data source

`simulated` -- there is **no live settings endpoint**. Two parts:

1. **Authorizations** (`MkSettingsPage`): whether an allowance is granted is an
   on-chain read (decentraland-transactions), not a Catalyst REST resource, so
   the granted/pending state is a SIMULATED faithful fixture. Contract addresses
   + network labels are the real mainnet/Polygon deployments; copy is verbatim
   from decentraland/marketplace `SettingsPage` + `Authorization`.
2. **Store** (`MkStoreSettingsEditor`): the storefront is a content-server STORE
   entity at pointer `urn:decentraland:off-chain:marketplace-stores:{address}`
   (`store/utils.ts` `getStoreUrn` + `fetchStoreEntity` ->
   `POST /content/entities/active`). The loader attempts that LIVE read first;
   the gateway at https://catalyst.example.com returns `[]` for store pointers (no
   off-chain store entities mounted -- verified live), so it degrades
   to the bundled fixture (`app/fixtures/marketplace-settings.json`). `source`
   (`live` | `fixture`) is reported in `mk_settings_viewed`.

Upstream shapes (recorded in the fixture `_source`): the flat `Store`
(`{ owner, cover, coverName, description, website, facebook, twitter, discord }`)
and link prefixes from decentraland/marketplace (master)
`webapp/src/modules/store/{types.ts,utils.ts}`; the deployable entity metadata
shape from decentraland/schemas `src/dapps/store.ts` `Store`.

## Deferred / simulated

- **Authorization toggles** -- flipping a radio grants/revokes an on-chain
  allowance (a wallet transaction) in the real app. Here the toggle is a local,
  in-component state change with no chain write (SIMULATED). The "pending" row
  (`off-eth`) shows the spinner state faithfully.
- **Store Save** -- saving deploys a signed STORE entity to the content server
  (`store/utils.ts` `deployStoreEntity`). That write is DEFERRED: clicking Save
  emits `mk_store_save_clicked` and the editor manages its own dirty/revert
  state, but no deploy is performed (read-only editor; clearly noted in the UI).
- **Cover upload** -- the real `CoverPicker` reads an uploaded `File` to a data
  URL; here the filled state uses an inline placeholder gradient (no binary
  asset / no upload), matching the ui3 component.
