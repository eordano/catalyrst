---
id: creator-hub-delete-project
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Gating the "Delete from My Scenes" action behind an explicit confirm modal --
    with a separate, opt-in "also delete files from my computer" checkbox and a
    permanent-delete warning -- lets creators remove clutter from My Scenes
    without accidentally destroying scene files on disk.
  because: >-
    Deletion is a desktop filesystem operation with two very different blast
    radii (drop from the list vs. permanently erase the folder). Surfacing the
    file-destruction choice as a deliberate, unchecked-by-default step with a
    red permanent-delete warning means the safe path (list-only removal) is the
    default and the destructive path is consciously chosen, so confirmed deletes
    rarely also wipe files unintentionally.
metric:
  primary: ch_delete_confirm_rate
  numerator: ch_delete_confirmed
  denominator: ch_delete_opened
  guardrails:
    - ch_delete_opened
    - ch_delete_cancelled
    - ch_delete_files_opted_in
decision:
  rule: >-
    Ship if ch_delete_confirm_rate (confirmed deletes / delete dialogs opened)
    holds with no rise in unintended file destruction -- ch_delete_files_opted_in
    must stay low relative to confirms and ch_delete_cancelled must not spike
    (creators are not bailing out, confused). Otherwise hold and revisit the
    copy/checkbox default.
experiment:
  key: ch_delete_project_confirm
  unit: session
  variants:
    - id: confirm_modal
      weight: 1
      flags:
        confirmModal: true
        fileCheckboxDefault: false
  baseline: 0.7
  mde: 0.05
  min_sample: 3000
---

# Delete a local scene project from My Scenes

The Creator Hub "My Scenes" grid lets a creator remove a local scene project via
the per-card kebab menu ("Delete from My Scenes"). The action is gated behind the
`DeleteProject` confirm modal: an opt-in "Also delete this scene's files from my
computer" checkbox (unchecked by default) plus a red permanent-delete warning that
only appears once the box is ticked. Confirming returns to the scenes list with the
project removed.

- **Data reality:** the scenes list is LIVE (owner-scoped Places via
  `loadCreatorScenes`; `?creator=` / the wallet cookie select the scope -- no
  fixture; only the modal copy comes from `delete-project.data.json`).
  Confirming performs a **REAL catalyst deletion**: the route resolves the live
  deployment (`resolveActiveScene`) and deploys a signed tombstone entity over
  its pointers (`deleteScene` -> `deployScene`), so `ch_delete_confirmed`
  carries `simulated: false` plus tombstone/status props. A never-deployed
  scene is refused honestly (`ch_delete_no_deployment` + explanatory error) and
  a rejected deploy surfaces the catalyst error (`ch_delete_failed`). The
  "also delete files from my computer" checkbox is **best-effort real**: when
  the project folder is connected through the File System Access handle store
  its contents are removed (`?local=deleted`); otherwise nothing is touched and
  the post-delete banner says the files were kept (`?local=kept`). The
  post-delete list drops the project via `?deleted=` and shows the tombstone id
  from `?tombstone=`.
- **Primary metric:** `ch_delete_confirm_rate` = `ch_delete_confirmed` / `ch_delete_opened`.
- **Guardrails:** `ch_delete_opened` (dialog volume), `ch_delete_cancelled`
  (abandon rate), `ch_delete_files_opted_in` (how often the destructive
  file-deletion path is chosen).
- **Events:**
  - `ch_delete_scenes_viewed` -- My Scenes grid rendered (`{ count }`).
  - `ch_delete_opened` -- confirm modal shown (`{ project_id, published }`).
  - `ch_delete_files_toggled` -- file-deletion checkbox flipped (`{ checked }`).
  - `ch_delete_files_opted_in` -- confirm submitted WITH file deletion checked
    (`{ project_id }`); guardrail for the destructive path.
  - `ch_delete_cancelled` -- modal dismissed without deleting (`{ project_id }`).
  - `ch_delete_confirmed` -- deletion committed (`{ project_id, delete_files,
    local_files, simulated: false, tombstone_id, overrode_pointers, status,
    replaced_entity }`); a REAL signed tombstone deploy, not a stub.
  - `ch_delete_no_deployment` -- confirm refused because the scene has no live
    deployment (`{ project_id, base }`).
  - `ch_delete_failed` -- the catalyst rejected the tombstone deploy
    (`{ project_id, status, error }`).
  - `ch_delete_done_viewed` -- post-delete scenes list rendered (`{ remaining }`).

Each journey step is URL-addressable via `?step` (scenes -> confirm -> deleted) and,
for the confirm step, `?project=<id>` selects which project's modal is shown.
