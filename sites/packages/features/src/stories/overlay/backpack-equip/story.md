---
id: bevy-overlay-backpack-equip
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided Backpack equip flow (browse -> select -> equip -> color -> review ->
    save) increases the share of opened Backpacks that reach the review/save step
    with a changed outfit, even with the profile deploy stubbed.
  because: >-
    Making the equip -> preview -> save path explicit (with a live preview diff and
    a clear save step) reduces uncertainty about what "save" does, so more players
    who open the Backpack and try on a wearable push through to saving their avatar
    instead of abandoning an opaque, single-pane editor.
metric:
  primary: cl_backpack_save_rate
  numerator: cl_backpack_review_reached
  denominator: cl_backpack_opened
  guardrails:
    - cl_backpack_opened
    - cl_backpack_inventory_empty
    - cl_backpack_equipped
decision:
  rule: >-
    Ship if cl_backpack_save_rate (cl_backpack_review_reached / cl_backpack_opened)
    improves by at least the MDE with no guardrail regression -- Backpack opens hold
    and the empty-inventory path stays graceful; otherwise hold.
experiment:
  key: cl_backpack_equip
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.35
  mde: 0.05
  min_sample: 4000
---

# Equip wearables in the Backpack and save the avatar

The Backpack overlay (`/client?panel=backpack`) breaks editing the avatar into
explicit steps: browse owned wearables by category, select one (see its rarity
in the right panel), equip it into its body slot (preview updates, the current
occupant is unequipped), pick skin/hair/eye colors, review the diff vs the
current profile, and save. This story tracks whether the guided flow increases
the share of opened Backpacks that reach review/save with a changed outfit.

- **Primary metric:** `cl_backpack_save_rate` =
  `cl_backpack_review_reached` / `cl_backpack_opened`.
- **Guardrails:** Backpack opens (`cl_backpack_opened`), the empty-inventory path
  (`cl_backpack_inventory_empty`), and equip volume (`cl_backpack_equipped`) must
  stay healthy.
- **Events:** `cl_backpack_opened` on mount, `cl_backpack_browsed`
  (`{empty}`), `cl_backpack_inventory_empty` (when the wallet owns nothing),
  `cl_backpack_selected` (`{urn, category, rarity}`), `cl_backpack_equipped`
  (`{urn, slot}`), `cl_backpack_color_changed` (`{kind, color}`),
  `cl_backpack_review_reached`, `cl_backpack_saved` (stub), and
  `cl_backpack_done`.

Data reality: the live owned inventory
(`/lambdas/collections/wearables-by-owner/{address}`) returns `[]` on this realm
(empty inventory), so the browse step shows the empty-state and the equip flow
runs against an upstream marketplace-api wearable catalog (captured in the
fixture). The equipped set seeds from the base avatar (the profile entity is
also empty). The final profile deploy (a signed profile entity POST to
`/content`) is **SIMULATED** in the XState wizard -- flow, states and metrics are
real; only the final commit is a clearly-noted stub. Noted as deferred.
