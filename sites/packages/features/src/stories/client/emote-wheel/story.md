---
id: client-emote-wheel
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A radial emote wheel reachable from the HUD increases expressive actions per
    session, raising the share of sessions that open the wheel and play an emote.
  because: >-
    A one-click, URL-addressable emote wheel (?panel=emote) makes expression a
    visible, low-friction affordance; sessions that open it convert to an emote
    play (cl_emote_played) sent through the engine bridge.
metric:
  primary: cl_emote_open_rate
  guardrails: []
experiment:
  key: client_emote_wheel
  unit: session
  variants:
    - id: radial-wheel
      weight: 100
      flags:
        radial: true
  baseline: 0.0
  mde: 0.05
decision:
  rule: >-
    Single-variant rollout. Ship if cl_emote_opened / cl_hud_loaded clears a
    healthy open-rate and cl_emote_played (the guardrail conversion to an actual
    emote) holds; otherwise revisit the wheel affordance.
---

# Client H04 -- Open the emote wheel

From the loaded HUD, clicking Emotes opens the radial emote wheel at the
URL-addressable `/client?panel=emote`. The ui3 EmoteWheel renders a radial ring
over the canvas; selecting a slot would send the play through the engine bridge
(`PlayEmote`). Absent the bridge (the norm here) the wheel still renders and
selection is local.

Journey:

1. `/client` (HUD loaded).
2. Click Emotes in the Sidebar -> `/client?panel=emote` (URL-addressable).
3. EmoteWheel renders the radial ring over the canvas.
4. `cl_emote_opened` fires on panel open.
5. Select a slot -> bridge `PlayEmote`; `cl_emote_played` fires (guardrail).

Primary metric `cl_emote_open_rate` = `cl_emote_opened` / `cl_hud_loaded`,
derivable from the events above.
