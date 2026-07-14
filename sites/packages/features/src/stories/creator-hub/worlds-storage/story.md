---
id: creator-hub-worlds-storage
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving World owners a single dashboard to inspect and edit their per-scene
    key/value + environment storage -- with their quota always visible --
    increases how often they engage with stored scene data instead of treating
    it as opaque.
  because: >-
    Today scene storage is a black box: owners deploy a scene and never see what
    keys it persisted or how much of their earned quota it uses. Surfacing the
    Select -> Scene -> Edit table next to the holdings-derived quota ("Your
    Storage") turns storage from an invisible backend detail into a manageable
    asset, so a higher share of dashboard views lead to an edit/clear action.
metric:
  primary: ch_worlds_storage_value_saved_rate
  guardrails:
    - ch_worlds_storage_viewed
    - ch_worlds_storage_asset_selected
    - ch_worlds_storage_cleared
decision:
  rule: >-
    Ship if ch_worlds_storage_value_saved_rate (value_saved per storage_viewed)
    improves by at least the MDE with no regression in ch_worlds_storage_viewed
    volume and no spike in ch_worlds_storage_cleared (accidental destructive
    clears); otherwise hold and revisit the dialog UX.
experiment:
  key: creator-hub-worlds-storage
  unit: session
  baseline: 0.18
  mde: 0.04
  min_sample: 2500
  variants:
    - id: storage-dashboard
      weight: 1
      flags:
        showQuotaModal: true
---

# Manage Worlds storage: per-scene key/value env data + storage quota

The Worlds storage tool of creator-hub / decentraland.org `/creator-hub/worlds-storage`. A World
owner or scene deployer picks a World (or LAND they operate), inspects the
per-scene **key/value** table (`world_storage`) and the **environment-variable**
keys (`env_variables`, values stay encrypted server-side), and can add / edit /
delete individual values or clear them all -- always alongside the
holdings-derived **storage quota** ("Your Storage").

Single shipping variant (`storage-dashboard`), `showQuotaModal: true`.

## Data source

Shapes follow upstream `decentraland/worlds-content-server`
(`src/types.ts` -> `WalletStats { wallet, dclNames[{name,size}], ensNames,
usedSpace, maxAllowedSpace }`) and the in-crate `catalyrst-world-storage`
JSON envelopes (`GET /values` -> `{ data:[{key,value}] }`, `GET /env` ->
`{ data:[{key}] }`, `GET /players` -> `string[]`, `GET /usage/world` ->
`{ usedBytes, maxTotalSizeBytes }`). Every `/world-storage/*` route is **404 on
the public nginx** and additionally requires an ADR-44 signed-fetch +
owner/deployer authorization, so the loader's live reads are expected to fail
and the tables render **empty** (`source: "empty"`; there is NO bundled
fixture -- nothing fabricated). The loader still issues the live reads so the
wiring is real, and the pickable **Worlds list is LIVE** (the wallet's NAMEs
via the Lambdas + `/world/{name}/about` scene counts).

**REAL (not simulated):** all mutations -- Add / Edit value, Delete row, Clear
all -- issue a REAL ADR-44 `signedFetch` `PUT`/`DELETE` against the gated
service. On this public realm those writes are expected to be rejected; the
failure is reported honestly via `ch_worlds_storage_*_failed` events rather
than fake-updating the table. The documented `*_saved`/`*_deleted`/`cleared`
events fire only when the service actually accepts the write.

## Metric

- **Primary:** `ch_worlds_storage_value_saved_rate` =
  `ch_worlds_storage_value_saved` / `ch_worlds_storage_viewed`
  (dashboard view -> an actual storage edit).
- **Guardrails:** `ch_worlds_storage_viewed` volume,
  `ch_worlds_storage_asset_selected` (did they pick an asset at all), and
  `ch_worlds_storage_cleared` (a destructive clear-all must not spike).

## Emitted events

- `ch_worlds_storage_viewed` -- on load (`{ step, source, world, namespace, worlds, lands }`).
- `ch_worlds_storage_asset_selected` -- picking a World/LAND to manage (`{ world, kind }`).
- `ch_worlds_storage_namespace_changed` -- switching Scene / Environment / Player (`{ namespace, world }`).
- `ch_worlds_storage_dialog_opened` -- opening the Add / Edit dialog (`{ mode, key, world }`).
- `ch_worlds_storage_value_saved` -- a value REALLY accepted by the signed write
  (`{ mode, key, world, namespace }`; `stub: true` variants mark the
  not-connected / no-value early-outs).
- `ch_worlds_storage_value_deleted` -- a row REALLY deleted (`{ key, world, namespace }`).
- `ch_worlds_storage_cleared` -- Clear-all REALLY accepted (`{ world, namespace, count }`).
- `ch_worlds_storage_value_save_failed` / `ch_worlds_storage_value_delete_failed`
  / `ch_worlds_storage_clear_failed` -- the gated service rejected the signed
  write (`{ ..., error }`); the expected outcome on the public realm.
- `ch_worlds_storage_quota_opened` -- opening the "Your Storage" quota panel (`{ world }`).

## Journey steps (URL-addressable)

1. `/creator-hub/worlds-storage` -- **select**: pick a World or LAND you own /
   operate (`ch_worlds_storage_viewed`).
2. `/creator-hub/worlds-storage?step=scene&world=vitsky.dcl.eth` -- **scene**: the
   per-scene key/value table with Edit/Delete row actions + Add / Clear All
   (`ch_worlds_storage_asset_selected`). `&namespace=env` shows the Environment
   variables view.
3. `/creator-hub/worlds-storage?step=edit&world=vitsky.dcl.eth&key=puzzle.state`
   -- **edit**: the Add / Edit value dialog (`ch_worlds_storage_dialog_opened`).
4. `/creator-hub/worlds-storage?step=clear&world=vitsky.dcl.eth` -- **clear**: the
   Clear-all confirm dialog (`ch_worlds_storage_cleared` on confirm).
5. `/creator-hub/worlds-storage?quota=1&world=vitsky.dcl.eth` -- the "Your
   Storage" quota PANEL (`ch_worlds_storage_quota_opened`), rendered in-page
   above the step body (scrimless panel variant; the step controls stay
   visible below). Opened by the "Your Storage" button; when the owner-gated
   wallet-stats data isn't available the panel says so honestly instead of
   rendering nothing.

## Deferred / honest failure modes

- Live `/world-storage/*` reads are 404 + signed-fetch gated on the public
  realm -> the tables render empty with `source: "empty"` (no fixture).
- Per-key VALUE bodies for env vars are never available client-side (encrypted
  `value_enc bytea`); the Environment view lists keys only and the Edit dialog
  starts blank for env keys.
- Writes are REAL signed requests; the gated service's rejections surface as
  `*_failed` events (the table is never fake-updated).
