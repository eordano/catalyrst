---
id: bevy-overlay-backpack-emotes
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided slot-first emote assigner (pick a slot -> browse owned emotes ->
    assign -> review -> save) increases the share of started emote edits that
    reach a saved loadout, even with the profile deploy simulated.
  because: >-
    Making the 10 numbered slots the explicit unit of work -- pick the slot, then
    bind an emote to it -- removes the ambiguity of "which slot am I editing?",
    so more players who open the Emotes tab push through to saving a loadout
    instead of abandoning a half-edited wheel.
metric:
  primary: cl_emotes_save_rate
  numerator: cl_emotes_saved
  denominator: cl_emotes_started
  guardrails:
    - cl_emotes_started
    - cl_emotes_slot_picked
decision:
  rule: >-
    Ship if cl_emotes_save_rate (cl_emotes_saved / cl_emotes_started) improves by
    at least the MDE with no guardrail regression (Emotes-tab open volume holds
    and players still reach the slot-pick step); otherwise hold.
experiment:
  key: cl_backpack_emotes
  unit: session
  variants:
    - id: slot_first
      weight: 1
      flags:
        slotFirst: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
---

# Assign emotes to the 10 numbered emote slots

The Backpack > Emotes tab (`?panel=backpack&tab=emotes`) lets a player bind one
of their owned emotes to each of the 10 numbered slots (1-9 then 0 -- the keys
pressed to play an emote in-world). This story walks the assign flow as an XState
wizard: pick a slot, browse owned emotes, assign one, review the 10-slot loadout,
then save. The save is a **SIMULATED** Profile-entity deploy -- there is no real
`/content/entities` POST; the flow, states and telemetry are real.

- **Primary metric:** `cl_emotes_save_rate` = `cl_emotes_saved` / `cl_emotes_started`.
- **Guardrails:** Emotes-tab open volume (`cl_emotes_started`) and the slot-pick
  step (`cl_emotes_slot_picked`) must stay healthy.
- **Events:** `cl_emotes_started` (tab opened), `cl_emotes_slot_picked` (`{slot}`),
  `cl_emotes_browse` (`{slot}`), `cl_emotes_assigned` (`{slot, urn}`),
  `cl_emotes_review` (`{count}`), `cl_emotes_saved` (`{count}`, **stub**),
  `cl_emotes_done`.

Data reality: the live lambda
`/lambdas/collections/emotes-by-owner/{address}` returns `200 []` (empty
inventory) on this realm, so the owned-emote catalog is derived from the
decentraland/schemas emote shapes (see
`app/fixtures/bevy-overlay-backpack-emotes.json` `_source`). The slot loadout
persisted to the avatar Profile entity is simulated. Noted as deferred.
