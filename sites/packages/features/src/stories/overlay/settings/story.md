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

# Bevy overlay -- client settings (Graphics / Sounds / Controls / Chat)

The in-client Settings menu, rendered as a HUD panel over the explore chrome and
opened with `?panel=settings`. It is a simple **loader + components** surface (no
multi-step machine): the loader mints a session id and returns the validated
settings catalog; the component composes the ui3 `SettingsView` strings (Toggle /
Slider / Dropdown atoms inside `ExploreChrome`) and re-renders for the active
`?tab`. The page renders fully without JS (the catalog is in the HTML);
analytics + persistence fire on the client.

## Data -- engine-backed over the bridge

Client settings are **engine state, not server state**: there is no Catalyst
endpoint, the bevy engine owns the values. Every module in the catalog names the
exact `SettingInfo.name` registered by bevy-explorer's `SettingBridgePlugin`
(`crates/system_bridge/src/settings/*.rs`); the catalog only fixes the section /
group layout and Unity-look labels (from unity-explorer's
`SettingsMenuConfiguration.asset`) plus first-paint seeds mirroring
`AppConfig::default()`. It lives in
`packages/data/src/lib/catalyst/overlay/settings-catalog.data.json` and is
validated by the zod schema in `packages/data/src/lib/catalyst/overlay/settings.ts`.

**Persistence is real**: on mount the panel sends `GetSettings` and renders the
engine's current values from the `settings` overlay push; changing a control
sends `SetSetting { name, value }` (enum settings by variant index, sliders by
raw engine value) and the engine echoes a fresh snapshot with the applied value,
persisting to its own config store (OPFS `config.json`). Without a bridge (plain
browser / Storybook) the controls render the seeded engine defaults and writes
are no-ops.

The Sound tab additionally renders a per-player voice mute list ("Voice Chat &
Streams -- Participants"): the engine streams the live LiveKit roster over the
`voiceParticipants` bridge push (blocked users filtered engine-side), each row
shows the participant's name, shortened address, speaking indicator and a Mute
toggle, and toggling sends `SetVoiceParticipantVolume { address, volume }` (0 =
muted, 1 = full). Without a bridge, or with nobody in voice, the list renders
the honest empty state "No one is in voice chat right now."

## Journey (URL-addressable)

- `/client?panel=settings` -- open Settings over the HUD (defaults to Graphics).
- `/client?panel=settings&tab=graphics` -- graphics sliders / toggles / dropdowns.
- `/client?panel=settings&tab=sounds` -- volume sliders + the per-player voice
  mute list.
- `/client?panel=settings&tab=controls` -- mouse sensitivity sliders + Point At dropdown.
- `/client?panel=settings&tab=chat` -- chat settings.

(The route also responds at `/bevy-overlay/settings...` as a standalone surface;
`?panel=settings` is honored so it can be reached the same way as the other HUD
panels.)

## Metrics

- **Primary:** `cl_settings_opened` -- fired once when the Settings panel mounts
  ({ tab }).
- **Guardrails:**
  - `cl_settings_tab_changed` -- a pill tab switch ({ tab }).
  - `cl_setting_changed` -- a control is changed ({ tab, key, kind, value }),
    fired alongside the `SetSetting` bridge write.

Single shipping variant (`pill-tabs`); the schema stays fully valid so the
readout tooling and deterministic bucketing work unchanged if a control arm is
added later.
