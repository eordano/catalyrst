---
id: create-hub-to-scenes
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a scenes-first Creator Hub home that one-clicks into the My Scenes
    list increases the share of hub visitors who reach the scene manager.
  because: >-
    Creators open the hub to work on scenes; a prominent Scenes card with a
    direct "See All" path removes a navigation step, so more hub sessions reach
    the scenes list instead of stalling on the landing screen.
metric:
  primary: ch_scenes_viewed_rate
  guardrails:
    - ch_home_viewed
    - ch_scenes_empty_viewed
experiment:
  key: ch_hub_to_scenes
  unit: session
  variants:
    - id: scenes_first
      weight: 1
      flags:
        scenesFirst: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if ch_scenes_viewed_rate improves by at least the MDE with no guardrail
    regression (hub viewing volume holds and the empty-state path stays usable);
    otherwise hold.
---

# Creator opens the hub and lands on their scenes

The Creator Hub home (`/create`) is the front door of the creator surface. This
story tracks whether a scenes-first hub that one-clicks into the My Scenes list
(`/create/scenes`) increases the share of hub visitors who reach the scene
manager.

- **Primary metric:** `ch_scenes_viewed_rate` = `ch_scenes_viewed` / `ch_home_viewed`.
- **Guardrails:** total `ch_home_viewed` volume and the empty-wallet path
  (`ch_scenes_empty_viewed`) must stay healthy.
- **Events:** `ch_home_viewed` on hub load, `ch_scenes_clicked` on the Scenes
  card action, `ch_scenes_viewed` on the scenes list, `ch_scenes_empty_viewed`
  when a zero-scene wallet hits the empty state.

Live data: deployed scenes are read from the live Catalyst Places API
(`/places/api/places`); a wallet's own scenes are filtered by creator. When no
scenes are present we render the ui3 empty state (graceful, never crashes).
