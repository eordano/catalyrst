---
id: studio-open-scene-lab
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Routing "open editor" into the modern Scene Lab (AI prompt + code + Bevy
    preview) instead of a static screen increases the rate of sessions that
    reach an interactive editor preview.
  because: >-
    An interactive studio that boots straight into a workspace and a live
    preview gives creators immediate feedback, so more sessions that open the
    studio actually toggle into the running preview rather than bouncing.
metric:
  primary: ch_studio_preview_rate
  guardrails:
    - ch_studio_opened
experiment:
  key: ch_studio_scene_lab
  unit: session
  variants:
    - id: scene_lab
      weight: 1
      flags:
        sceneLab: true
  baseline: 0.5
  mde: 0.05
  min_sample: 5000
decision:
  rule: >-
    Ship if ch_studio_preview_rate improves by at least the MDE with no
    guardrail regression (studio-open volume and workspace-render rate hold);
    otherwise hold.
---

# Open the modern Scene Lab studio and reach the live preview

"Open in Studio" routes into the modern Scene Lab -- an AI prompt panel + a code
editor + an embedded Bevy preview -- instead of a static editor screen. This
story tracks whether that increases the rate of sessions that reach an
interactive editor preview.

- **Primary metric:** `ch_studio_preview_rate` = `studio_preview_viewed` / `ch_studio_opened`.
- **Guardrails:** studio-open volume (`ch_studio_opened`) and the workspace
  render step (`studio_workspace_viewed`) must hold.
- **Events:** `ch_studio_opened` on entry, `studio_loaded` after the boot/first
  run, `studio_workspace_viewed` when the two-panel workspace renders,
  `studio_preview_viewed` when the Preview tab is shown
  (`/studio/editor?view=preview`).

The studio is presentational/stubbed (Scene Lab is not a live Catalyst service);
the journey steps are URL-addressable for screenshotting.
