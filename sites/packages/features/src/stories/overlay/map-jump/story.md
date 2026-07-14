---
id: bevy-overlay-map-jump
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A full-bleed world map where players filter pins by category, inspect a
    parcel's PlaceInfoPanel, then confirm before teleporting increases the share
    of map sessions that complete a jump, versus a one-tap pin that teleports
    immediately.
  because: >-
    Letting players preview a destination (name, coords, live users, rating) and
    confirm the target -- with a set-as-home option -- reduces mis-jumps and
    hesitation, so more players who open the map follow through to an in-world
    arrival instead of closing the map or bouncing straight back out.
metric:
  primary: cl_map_jump_rate
  guardrails:
    - cl_map_opened
    - cl_map_pin_selected
experiment:
  key: cl_map_jump
  unit: session
  variants:
    - id: navmap
      weight: 1
      flags:
        navmap: true
        confirmStep: true
  baseline: 0.35
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if cl_map_jump_rate improves by at least the MDE with no guardrail
    regression (map opens hold and pin-select volume does not drop); otherwise
    hold. The teleport itself is simulated, so the readout judges the funnel up
    to the jump intent, not the in-world load.
---

# Pick a destination on the world map and jump in

The bevy-overlay Navmap (`?panel=map`) is a full-bleed parcel tilemap with a
category-pill row + search across the top and a LAYERS drawer (Live Events /
POI / Mini-Games / People). Pins are positioned from LIVE catalyst place data
(`GET /places/api/places` -- the same surface as `/places`); each place's
`base_position` is its tile coordinate. Selecting a pin opens the
**PlaceInfoPanel** (name, coords, users, rating, creator), from which the player
confirms a jump target (with a set-as-home option) and teleports.

This story tracks whether the preview-and-confirm navmap increases the share of
map sessions that complete a jump.

- **Primary metric:** `cl_map_jump_rate` = `cl_map_jump` / `cl_map_opened`.
- **Guardrails:** map-open volume (`cl_map_opened`) and pin-select volume
  (`cl_map_pin_selected`) must stay healthy.
- **Events (per-transition):**
  - `cl_map_opened` -- Navmap opened (`?panel=map`).
  - `cl_map_filtered` (`{filter}`) -- a LAYERS category filter applied.
  - `cl_map_pin_selected` (`{place_id, coords}`) -- a parcel pin selected ->
    PlaceInfoPanel.
  - `cl_map_confirm_reached` (`{coords, set_home}`) -- jump target confirmed.
  - `cl_map_jump` (`{place_id, coords, jump_url, simulated:true}`) -- the
    **SIMULATED** teleport fires (bridge Teleport / `decentraland.org/jump/` deep link).
  - `cl_map_jump_done` -- in-world arrival confirmation.

## Data reality / what's simulated

The map tile **raster** is GPU/engine-only: catalyst `/v1/tiles` and `/v2/tiles`
both 404 on the public edge, so the tilemap is the ui3 CSS parcel-grid
placeholder (per `Map.jsx`) with live pins drawn over it. Pin **metadata** is
real (live `/places/api/places`, fixture fallback). The **teleport** is
simulated in the XState machine -- there is no bridge to the Unity/Bevy engine
here -- so it resolves a `decentraland.org/jump/` deep link after a short delay
rather than entering a world. Flow, states, deep-links, and telemetry are real;
the final in-world commit is a clearly-noted stub.
