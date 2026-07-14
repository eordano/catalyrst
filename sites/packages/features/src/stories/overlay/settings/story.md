---
id: bevy-overlay-settings
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing the in-client Settings menu as a tabbed, deep-linkable HUD panel
    (Graphics / Sounds / Controls / Chat) lets players tune the experience to
    their hardware and comfort, raising the share of sessions that change at
    least one setting and stick with it.
  because: >-
    Players who never open Settings run on defaults that may stutter on their
    GPU or feel uncomfortable (mouse sensitivity, chat noise); a clear pill-tab
    layout with sensible defaults and instant client-side persistence removes the
    friction of finding and applying a fix, so more sessions adjust a setting
    instead of bouncing or tolerating a bad default.
metric:
  primary: cl_settings_opened
  guardrails:
    - cl_settings_tab_changed
    - cl_setting_changed
experiment:
  key: cl_settings_panel
  unit: session
  variants:
    - id: pill-tabs
      weight: 1
      flags:
        showPillTabs: true
        persistLocal: true
  baseline: 0.2
  mde: 0.03
  min_sample: 4000
decision:
  rule: >-
    Ship if sessions that fire cl_settings_opened go on to fire cl_setting_changed
    at or above the MDE over baseline, with no regression in cl_settings_tab_changed
    (players can still navigate between sections); otherwise hold.
---

# Bevy overlay — client settings (Graphics / Sounds / Controls / Chat)

The in-client Settings menu, rendered as a HUD panel over the explore chrome and
opened with `?panel=settings`. It is a simple **loader + components** surface (no
multi-step machine): the loader mints a session id and returns the validated
settings catalog; the component composes the ui3 `SettingsView` strings (Toggle /
Slider / Dropdown atoms inside `ExploreChrome`) and re-renders for the active
`?tab`. The page renders fully without JS (the catalog is in the HTML);
analytics + persistence fire on the client.

## Data — SIMULATED / local-only

Client settings are **engine state, not server state**. A scan of catalyrst
(governance / presence / price + the explorer-api lambdas) confirms there is **no
settings endpoint**, and the real client persists these to disk / a local store,
never to a Catalyst service. So the inventory + default values are derived
faithfully from the ui3 `SettingsView` composition (`Settings.jsx` +
`Controls.jsx`), which mirrors the unity-explorer `SettingsView` prefab strings
(the upstream `.asset` path 404s — Unity prefab/asset files are not in the public
raw tree; recorded in the fixture `_source`). They live in
`app/fixtures/bevy-overlay-settings.json` and are validated by a small zod schema
in `app/lib/catalyst/settings.ts`.

**Persistence is SIMULATED**: changing a control writes to `localStorage` under
`dcl:settings` and (when present) pushes to `window.dclBridge` — there is no
server round-trip. In this app the bridge is absent, so writes degrade to
localStorage only. This is clearly noted in the route and the fixture.

## Journey (URL-addressable)

- `/client?panel=settings` — open Settings over the HUD (defaults to Graphics).
- `/client?panel=settings&tab=graphics` — graphics sliders / toggles / dropdowns.
- `/client?panel=settings&tab=sounds` — volume sliders.
- `/client?panel=settings&tab=controls` — mouse sensitivity sliders + Point At dropdown.
- `/client?panel=settings&tab=chat` — chat settings.

(The route also responds at `/bevy-overlay/settings…` as a standalone surface;
`?panel=settings` is honored so it can be reached the same way as the other HUD
panels.)

## Metrics

- **Primary:** `cl_settings_opened` — fired once when the Settings panel mounts
  ({ tab }).
- **Guardrails:**
  - `cl_settings_tab_changed` — a pill tab switch ({ tab }).
  - `cl_setting_changed` — a control is changed ({ tab, key, kind, value }); this
    is also the event that records the SIMULATED client-side persistence.

Single shipping variant (`pill-tabs`); the schema stays fully valid so the
readout tooling and deterministic bucketing work unchanged if a control arm is
added later.
