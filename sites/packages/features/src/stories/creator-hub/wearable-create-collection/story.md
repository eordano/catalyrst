---
id: creator-wearable-create-collection
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step create-collection wizard inside the Creator Hub
    (name -> type -> items -> review the MANA publish cost -> submit) increases
    the share of creators who start a wearables collection and reach the submit
    step, where the on-chain publish cost is made explicit before signing.
  because: >-
    First-time creators stall when collection setup, item rarity/category, and
    the per-item MANA publication fee are conflated into one opaque form.
    Splitting it into legible, URL-addressable steps inside the familiar Creator
    Hub chrome and surfacing the cost estimate before the submit step reduces
    uncertainty, so more creators who start push through to submit instead of
    abandoning.
metric:
  primary: cwc_create_collection_submit_rate
  numerator: bd_create_collection_submitted
  denominator: bd_create_collection_started
  guardrails:
    - bd_create_collection_started
    - bd_create_collection_review_reached
experiment:
  key: cwc_create_collection_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.42
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if cwc_create_collection_submit_rate improves by at least the MDE with
    no guardrail regression (create-start volume holds and the review step still
    converts); otherwise hold.
---

# Create a wearables collection (Creator Hub wizard)

The create-collection wizard (`/create/wearables/collections/new`) breaks
new-collection setup into explicit, URL-addressable steps inside the Creator Hub
desktop chrome (`CreatorHubChrome`):

1. `?step=name` -- name the collection (validation) -- emits
   `bd_create_collection_started { type }` (entry) + `bd_create_collection_named`.
2. `?step=items` -- upload wearable files (`.zip`, `.gltf`, `.glb`, `.png` via
   dropzone/file picker) -- `bd_create_collection_items_added { count }`.
3. `?step=review` -- review uploads + the **MANA publish-cost estimate** --
   `bd_create_collection_review_reached`.
4. `?step=submit` -- sign + submit (SIMULATED on-chain mint of the collection
   contract) -- **`bd_create_collection_submitted`** (the primary funnel event).
5. `?step=done` -- created (stub) -> link to
   `/create/wearables/collections/{id}` -- `bd_create_collection_completed`.

Every collection is a **Standard Collection** by default -- there is no type
step. The third-party (linked) path stays discoverable via a quiet footnote on
the name step ("Managing a registered third-party collection?") which sets
`?type=linked`; direct links with `?type=linked` / `?type=third_party` also
work and zero out the per-item fee.

- **Primary metric:** `cwc_create_collection_submit_rate` =
  `bd_create_collection_submitted` / `bd_create_collection_started`.
- **Guardrails:** create-start volume (`bd_create_collection_started`) and the
  review step (`bd_create_collection_review_reached`) must stay healthy.
- **Events:** `experiment_exposed` (loader, on render), `bd_create_collection_started
  {type}` + `bd_create_collection_named`, `bd_create_collection_items_added {count}`,
  `bd_create_collection_review_reached`, **`bd_create_collection_submitted`**
  (entry to submit/sign), and `bd_create_collection_completed` (stub) on done.

**Data reality / simulated:** the on-chain collection-contract creation/mint and
the live per-item MANA publication-fee quote are simulated -- there is no signer
or provider here, and the builder-server collection write is auth-gated. The UI
says so on the submit and done panels ("On-chain mint is SIMULATED", "created
(stub)"). Shapes mirror `decentraland/schemas` (Rarity, maxSupply,
WearableCategory) and `decentraland/builder-server` (CollectionAttributes,
CollectionTypeFilter); there is no fixture -- the items step is a real File-API
dropzone and nothing pre-seeds mock items. The flow, validation, cost math
(perItem MANA x item count) and telemetry are real; the final submit/commit is
a clearly-noted stub whose done link opens the (empty, sim-id) collection
detail page.
