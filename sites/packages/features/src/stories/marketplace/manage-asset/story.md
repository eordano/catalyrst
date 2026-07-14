---
id: marketplace-manage-asset
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    An owner who lands on a single "manage" hub for one of their assets -- seeing
    the asset, its current listing state, and Sell / Transfer / Cancel actions in
    one place -- engages an action more often than when those actions are scattered
    across separate pages. Surfacing all owner actions on one screen (treatment)
    lifts the rate of action clicks per managed-asset view over a layout that only
    shows the asset detail (control).
  because: >-
    Owners arrive with intent (list, move, or de-list an item) but hesitate when
    the next step is unclear or buried. A single action hub that reflects the live
    listing state (Sell when idle, Cancel when listed) removes the navigation tax
    and makes the obvious next action one click away.
metric:
  primary: mk_manage_action_clicked rate per mk_manage_asset_viewed
  guardrails:
    - mk_manage_asset_viewed
experiment:
  key: marketplace_manage_action_hub
  unit: session
  variants:
    - id: control
      weight: 50
      flags:
        actionHub: false
    - id: treatment
      weight: 50
      flags:
        actionHub: true
  baseline: 0.18
  mde: 0.05
decision:
  rule: >-
    Ship treatment if mk_manage_action_clicked / mk_manage_asset_viewed is higher
    than control with 95% confidence and mk_manage_asset_viewed does not regress;
    otherwise keep control.
---

# Marketplace -- Manage an owned asset (sell / transfer / cancel action hub)

Owner opens `/marketplace/manage?id=<contractAddress>-<tokenId>` for an asset
they hold. The loader fetches the owned NFT with its inline active listing
(`GET /credits/v1/nfts?owner=&contractAddress=&tokenId=`), resolves the
`marketplace_manage_action_hub` variant, emits exposure, and renders the ui3
`MkManageAssetPage` wrapped in `AssetActionLayout`. The page reflects the live
listing state: **Sell** when the asset is idle, **Cancel listing** when an order
is open, plus **Transfer** in both states.

This is a `spec` story: loader + components rendering from `loaderData` (no
XState wizard). The action buttons are deep-links into the dedicated flow stories
(sell / transfer / cancel-listing) and DO NOT mutate on-chain state here -- the
manage hub is read-only; commits live in those flows.

## Journey + metrics

- Load `/marketplace/manage?id=<id>` (step `load-owned-asset`) ->
  `experiment_exposed` (trackExposure) +
  `mk_manage_asset_viewed { item_id, listed, has_rental, network }`.
- Action hub shown (step `show-actions`): Sell / Transfer / Cancel buttons reflect
  the live listing state.
- Click an action (step `deep-link-to-flow`) ->
  `mk_manage_action_clicked { item_id, action }` where `action` is one of
  `sell | transfer | cancel`, then navigates to the flow route.

## A/B

`actionHub` (control=false / treatment=true) controls whether the consolidated
Sell / Transfer / Cancel action hub is surfaced. `?variant=marketplace_manage_action_hub:<id>`
is honored as a PREVIEW-only QA override.

## Deferred / simulated

- The owned-asset source is `/credits/v1/nfts?owner=` (the spec's
  `/users/{address}/wearables` returns an empty set for arbitrary owners; the
  `?owner=` nfts query carries the inline `order` so it covers both halves of the
  declared dataSource in one call). A fixture
  (`app/fixtures/marketplace-manage-asset.json`) backs SSR when Catalyst is down
  or the owner holds nothing.
- Action buttons are pure deep-links; on-chain sell/transfer/cancel are owned by
  the dedicated flow stories and are not executed from this hub.
