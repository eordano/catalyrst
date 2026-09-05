---
id: creator-hub-world-settings
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, tab-by-tab World Settings wizard (details -> layout -> misc ->
    review -> save) increases the share of opened settings sessions that reach a
    saved write, even with the metadata commit simulated.
  because: >-
    Splitting a published World's settings into explicit, URL-addressable tabs
    with a single review step that names every pending change removes the
    ambiguity of the all-at-once modal, so more owners who open settings push
    through to a confident Save instead of abandoning unsaved edits.
metric:
  primary: ch_world_settings_save_rate
  numerator: ch_world_settings_saved
  denominator: ch_world_settings_opened
  guardrails:
    - ch_world_settings_opened
    - ch_world_settings_discarded
decision:
  rule: >-
    Ship if ch_world_settings_save_rate improves by at least the MDE with no
    guardrail regression (settings-open volume holds and the discard rate does
    not climb); otherwise hold.
experiment:
  key: ch_world_settings_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.5
  mde: 0.05
  min_sample: 3000
---

# Edit a published World's settings (details / layout / misc + review + save)

The Creator Hub "World Settings" modal edits a published World's metadata. This
story breaks it into explicit, URL-addressable steps that mirror the real
tabbed dialog and add a single review gate before the write:

1. **details** -- DETAILS tab: World Title, Description, Thumbnail, Categories.
2. **layout** -- LAYOUT tab: the X/Y parcel layout (multi-scene world map +
   published scenes list).
3. **misc** -- MISC. tab: spawn coordinate (X/Y), skybox Auto/fixed-hour offset,
   Single Player toggle, Show in Places toggle.
4. **review** -- unsaved-changes banner that names every pending edit with
   Discard / Save changes.
5. **save** -- a simulated `PUT /world/:name/settings` writes the new metadata
   back to the World; success returns the updated `WorldSettings`.

The story tracks whether the wizard increases the share of opened settings
sessions that reach a saved write.

- **Primary metric:** `ch_world_settings_save_rate` =
  `ch_world_settings_saved` / `ch_world_settings_opened`.
- **Guardrails:** settings-open volume (`ch_world_settings_opened`) and the
  discard rate (`ch_world_settings_discarded`) must stay healthy.
- **Events:** `ch_world_settings_opened` on first step,
  `ch_world_settings_tab_viewed` (`{ tab }`) per tab,
  `ch_world_settings_changed` (`{ tab, field }`) when a field is edited,
  `ch_world_settings_review_reached`, `ch_world_settings_discarded`,
  `ch_world_settings_saving`, `ch_world_settings_saved` (stub, `{ fields }`).

Data reality: `worlds-content-server` has no public read endpoint for a single
owner's WorldSettings (the GET is auth-gated and `worlds/*` is not publicly
served), so the World record is a faithful instance derived from the upstream
`WorldSettings` / `WorldInfo` / `WorldRuntimeMetadata` shapes
(`decentraland/worlds-content-server/main/src/types.ts`) plus the scene-level
`worldConfiguration.skyboxConfig` / `spawnPoint` shapes
(`decentraland/schemas/main`), written to
`app/fixtures/creator-hub-world-settings.json`. The settings WRITE is
**SIMULATED** in the XState wizard (flow, states and metrics are real, only the
final metadata commit is a clearly-noted stub). Noted as deferred.
