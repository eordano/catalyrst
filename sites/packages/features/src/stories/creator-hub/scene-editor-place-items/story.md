---
id: creator-hub-scene-editor-place-items
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided in-inspector "place an item" flow (open the assets dock -> drop a
    model into a new entity -> set its transform -> add a component -> autosave)
    raises the share of editing sessions that successfully place at least one
    item and persist the scene, even with the save stubbed.
  because: >-
    The scene editor's power is hidden behind an empty viewport and three
    disconnected docks (hierarchy, assets, inspector). Walking creators through
    the place -> transform -> add-component -> save loop once makes the core
    authoring gesture legible, so more first-time editing sessions reach a saved
    scene instead of stalling on an empty hierarchy.
metric:
  primary: ch_editor_place_rate
  numerator: ch_editor_saved
  denominator: ch_editor_opened
  guardrails:
    - ch_editor_opened
    - ch_editor_asset_searched
decision:
  rule: >-
    Ship if ch_editor_place_rate (sessions reaching ch_editor_saved over sessions
    with ch_editor_opened) improves by at least the MDE with no guardrail
    regression (editor-open volume holds and asset search stays healthy);
    otherwise hold.
experiment:
  key: ch_editor_place_items
  unit: session
  variants:
    - id: guided
      weight: 1
      flags:
        guided: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
---

# Edit a scene: place items / add entities and components in the inspector

The Creator Hub scene editor (`/creator-hub/scene-editor`) floats the editor
chrome over the bevy viewport: a top toolbar, a left dock that toggles between
the entity Hierarchy and the Assets catalog, and a right Inspector for the
selected entity. This story walks the core authoring loop as an XState wizard:

1. **editor** -- open the editor over the viewport (toolbar + hierarchy + inspector).
2. **browse-assets** -- open the Assets catalog dock and search a model.
3. **place-item** -- add the asset as a new entity in the hierarchy (New Entity dialog).
4. **transform** -- set the new entity's Transform position / rotation / scale.
5. **add-component** -- add a component via the inspector add-component picker.
6. **save** -- autosave / simulated write of the scene composite (CRDT).

### Modify phase (edit an existing real entity)

The editor opens on the **real** deployed scene at pointer `0,0` (Genesis Plaza)
by default, so the hierarchy is populated with that scene's actual entities
(Admin Tools, Video Screen, the theatre data sources). Besides placing new items,
the wizard supports a **modify** branch that operates on those real entities:

7. **modify** -- pick a real entity from the loaded scene, then rename it, move it
   (Transform), add a component, or delete it, and persist -- all reachable from
   the **editor** step. Rename/move/add-component stay on the step; delete and
   "save changes" route into the same **save** step as the place-item funnel.

Each step is URL-addressable via `?step=`
(open|assets|place|transform|component|modify|save) so any screen can be
deep-linked / screenshotted. Deep links hydrate the machine AT that step via an
XState snapshot, so none of the funnel telemetry double-fires.

- **Primary metric:** `ch_editor_place_rate` = `ch_editor_saved` / `ch_editor_opened`.
- **Guardrails:** editor-open volume (`ch_editor_opened`) and asset-search
  health (`ch_editor_asset_searched`).
- **Events:** `ch_editor_opened` on entry, `ch_editor_assets_browsed`,
  `ch_editor_asset_searched` (`{query}`), `ch_editor_entity_created`
  (`{asset_id, asset_name, entity}`), `ch_editor_transform_set` (`{axis}`),
  `ch_editor_component_added` (`{component, on}`), `ch_editor_entity_modified`
  (`{entity, name}`), `ch_editor_entity_renamed` (`{entity, name}`),
  `ch_editor_entity_deleted` (`{entity, name}`), `ch_editor_saved`
  (`{mode, entity, component, deleted, stub:true}`).

## Data reality (sources + what is simulated)

The editor is seeded from a LIVE deployment:
`POST https://catalyst.example.com/content/entities/active {"pointers":["0,0"]}`
returns the active scene entity (title + content mappings) which seeds the
editor header; on any failure we fall back to
`app/fixtures/creator-hub-scene-editor-place-items.json`. The entity/component
CRDT model (Transform id=1 with position/rotation/scale/parent, the
`core-schema::Name` component, and the addable-component registry --
GltfContainer / MeshRenderer / MeshCollider / Material / VisibilityComponent /
TextShape / AudioSource / ...) is derived from
`decentraland/js-sdk-toolchain @dcl/ecs` and the entity envelope from
`decentraland/schemas/src/platform/entity.ts`.

The content server is **read-only** over the public realm (a real deploy needs a
signed AuthChain POST to `/content/entities`, which is auth-gated). So the
EDITS -- placing the item, writing the Transform, adding the component, the
modify-phase rename / move / delete on a real entity, and the final composite
save -- are **SIMULATED** in the XState machine. The flow, states, deep-linking,
the loaded scene + entity list, and telemetry are all real (the modify phase
reads the genuine entity ids/names from the live composite); only the
persistence step is a clearly-noted stub.
