---
id: bevy-overlay-outfit-save
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided save-outfit flow (open a saved-outfit slot -> name it -> capture
    the current equipped set -> save) increases the share of started saves that
    reach the save step, even with the write simulated.
  because: >-
    Splitting "save outfit" into explicit, legible steps -- and making the
    NAME-gate on extra slots obvious before the save -- reduces uncertainty about
    what will be stored where, so more players who start saving an outfit push
    through instead of bailing at an opaque single-shot save.
metric:
  primary: cl_outfit_save_rate
  numerator: cl_outfit_saved
  denominator: cl_outfit_save_started
  guardrails:
    - cl_outfit_save_started
    - cl_outfit_slot_gated
decision:
  rule: >-
    Ship if cl_outfit_save_rate improves by at least the MDE with no guardrail
    regression (save-start volume holds and the NAME-gate keeps blocking extra
    slots without a NAME); otherwise hold.
experiment:
  key: cl_outfit_save_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
---

# Save a named outfit to an outfit slot

The Backpack > SAVED OUTFITS surface lets a player snapshot their current
equipped set into one of ten outfit slots. The wizard breaks it into explicit,
URL-addressable steps: open the SAVED OUTFITS sub-tab (SAVE OUTFIT card + Empty
Slot silhouettes), **name** the outfit for a chosen slot, **capture** the
current equipped set, **save** (NAME-gated for extra slots), then a confirmation.

- **Primary metric:** `cl_outfit_save_rate` = `cl_outfit_saved` / `cl_outfit_save_started`.
- **Guardrails:** save-start volume (`cl_outfit_save_started`) and the NAME-gate
  (`cl_outfit_slot_gated` must keep firing when an extra slot is picked without a
  NAME).
- **Events:** `cl_outfit_save_started` (open slot), `cl_outfit_named` (`{slot}`),
  `cl_outfit_captured` (`{slot, wearables}`), `cl_outfit_slot_gated`
  (`{slot, reason}` -- extra slot without a NAME), `cl_outfit_saved`
  (`{slot, name, simulated:true}` -- the save step), `cl_outfit_save_completed`
  (confirmation).

Data reality: `catalyst:/lambdas/profiles` (POST) is live-200 but returns
`{ avatars: [] }` (no deployed profile on this gateway), so the current equipped
set is seeded from the bundled fixture (schemas default base avatar). Outfits
are stored on the profile entity (`Outfits = { outfits[], namesForExtraSlots }`,
per decentraland/schemas `src/platform/outfits/outfits.ts`). The five free slots
are open; the extra five are NAME-gated via `namesForExtraSlots` (empty here ->
locked). The actual save is **SIMULATED** (the content write is read-only at
this gateway) -- flow, states, metrics and the NAME-gate are real.
