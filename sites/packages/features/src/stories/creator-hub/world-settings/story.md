---
id: creator-hub-world-settings
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    World settings with persistent edits across tabs and explicit save/discard
    actions increase the share of owners who successfully update their World.
  because: >-
    Splitting a published World's settings into explicit, URL-addressable tabs
    with persistent form values and visible save status removes the
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

# Edit a published World's settings

Details, Layout and Misc. tabs retain edits while navigating between them.
Existing values come from `GET /world/:name/settings`. Title, description,
categories, spawn coordinates, skybox time, single-player mode and listing
visibility are controlled inputs. Thumbnail uploads accept PNG/JPEG/GIF/WebP,
up to 1 MB. Save sends only changed fields through authenticated multipart
`PUT /world/:name/settings`; the returned stored values become the new baseline.

Save succeeds only after the backend accepts the write. Validation, expired
sessions, forbidden writes and service failures preserve the pending edits and
show a recoverable error. Discard restores the saved baseline. Leaving with
unsaved edits prompts the owner; changing tabs does not. Collaborators retain
their authorized scene-removal controls on Layout without metadata editing.

The server verifies the wallet, World ownership/deployment permission, file
contents and limits, and the spawn point's position within the published World.
The UI does not infer save success from a timer or local storage.

Primary metric: `ch_world_settings_saved / ch_world_settings_opened`.
Saved events contain the changed fields and `stub: false`. The opened,
tab-viewed, changed, discarded and saving events preserve the existing metric
contract. The old state-machine examples remain test fixtures, not the runtime
save path.
