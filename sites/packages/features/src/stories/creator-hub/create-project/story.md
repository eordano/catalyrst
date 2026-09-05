---
id: creator-hub-create-project
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided multi-step create-project wizard (name -> path -> template ->
    scaffold) increases the share of started new-scene flows that reach a
    scaffolded ("created") project written to the creator's disk.
  because: >-
    Splitting project creation into explicit, validated steps -- a checked
    project name, a free/writable path with a folder picker, and a clear
    template choice that defaults to Empty Scene -- removes the ambiguity of the
    single create modal, so more creators who begin a new scene push through to
    a scaffolded project instead of abandoning at an opaque form.
metric:
  primary: ch_create_project_completion_rate
  numerator: ch_create_project_completed
  denominator: ch_create_project_started
  guardrails:
    - ch_create_project_started
    - ch_create_project_path_invalid
decision:
  rule: >-
    Ship if ch_create_project_completion_rate improves by at least the MDE with
    no guardrail regression (new-scene start volume holds and the invalid-path
    rate does not climb); otherwise hold.
experiment:
  key: ch_create_project_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
---

# Create a new scene project (name + path + template)

The Creator Hub "Create Project" flow (`/creator-hub/create-project`) breaks
starting a new SDK7 scene into explicit, URL-addressable steps:

1. **name** -- Project Name input, validated non-empty.
2. **path** -- Project Path input + folder picker, validated free/writable.
3. **template** -- pick a starting template (defaults to **Empty Scene**).
4. **scaffold** -- a REAL `npx @dcl/sdk-commands init`-equivalent generates a
   valid SDK7 project (`scene.json`, `main.composite`, `package.json`,
   `tsconfig.json`, `src/index.ts`, `.gitignore`, `README.md`) and writes it to
   the creator's disk via the File System Access API (in-place directory write on
   Chromium; per-file Blob download elsewhere).
5. **created** -- success; open the new project in Studio or My Scenes.

Only **name** and **template** are URL-resumable; **scaffold**, **created**
and **error** are run-scoped, machine-owned states -- the URL sync skips them
so a fast scaffold cannot race the router into remounting the wizard back to
naming, and the success screen always renders.

This story tracks whether the wizard increases the share of started create-project
flows that reach a scaffolded project.

- **Primary metric:** `ch_create_project_completion_rate` =
  `ch_create_project_completed` / `ch_create_project_started`.
- **Guardrails:** new-scene start volume (`ch_create_project_started`) and the
  invalid-path rate (`ch_create_project_path_invalid`) must stay healthy.
- **Events:** `ch_create_project_started` on first step, `ch_create_project_name_set`
  (`{ name }`), `ch_create_project_path_set` (`{ path }`),
  `ch_create_project_path_invalid` (`{ path }`),
  `ch_create_project_template_selected` (`{ template }`),
  `ch_create_project_scaffolding`, `ch_create_project_completed`
  (`{ files, written, via, folder }`).

Data reality: the scaffold shapes (scene.json / main.composite / package.json /
tsconfig.json / src/index.ts) follow `decentraland/sdk7-scene-template` and the
SDK7 composite wire format; the on-disk write is **REAL** -- the wizard's Create
action generates the project from the user-typed name + parcel layout + chosen
template (`buildScaffoldFiles`) and persists every file through `lib/fs/disk`
(`writeScaffoldFiles`). A dismissed folder picker is a routine cancel, not a
failure: the wizard stays on the step the user was on (naming/templates) so
they can just try again -- it is never reported as an error nor as a created
project. Proof: `npm test`
(`app/lib/fs/scaffold-project.test.ts`) + `scripts/prove-scaffold-project.mts`
(writes to a real temp dir and reads the files back).
