---
id: client-hud-overlay
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Rendering the full ui3 HUD as a DOM layer over the engine canvas gives players
    an instantly-usable, server-rendered shell while the 3D world streams in,
    raising the share of client sessions that reach a ready HUD.
  because: >-
    The resting HUD (Sidebar, Minimap, ConnectionStatus, Chat) is server-rendered
    from the realm /about and works before the engine attaches, so players see a
    usable interface immediately instead of a blank canvas -- every loaded session
    fires cl_hud_loaded exactly once on overlay mount.
metric:
  primary: cl_hud_loaded_rate
  guardrails:
    - cl_explore_opened
experiment:
  key: client_hud_overlay
  unit: session
  variants:
    - id: overlay
      weight: 100
      flags:
        domOverlay: true
  baseline: 0.0
  mde: 0.05
decision:
  rule: >-
    Single-variant rollout. Ship if cl_hud_loaded fires for the overwhelming
    majority of /client sessions (overlay mounts reliably) with no error spike;
    otherwise investigate SSR/hydration of the overlay.
---

# Client H01 -- HUD overlay loads over the engine canvas

The `/client` route renders a React DOM overlay layered OVER a canvas backdrop
(`<canvas id="bevy-canvas">`), composing the ui3 HUD islands as DOM -- NOT as an
in-world 3D scene. This canvas is an SSR placeholder/backdrop stub: the
bevy-explorer engine is never instantiated in this app, so nothing renders into
it here. In the real deployed wasm build the engine mounts to `<canvas
id="mygame-canvas">` (the `bevy-canvas` id used here is an SSR-only placeholder,
not the live engine mount). The loader fetches the realm `/about` server-side and
seeds ConnectionStatus + the Minimap label; the resting HUD (Sidebar, Minimap,
ConnectionStatus, VoiceChat, Chat) renders with JS disabled.

Journey:

1. `GET /client` -- SSR shell: canvas backdrop + DOM overlay.
2. Realm `/about` loaded server-side -> ConnectionStatus + Minimap seeded.
3. Sidebar + Minimap + Chat resting state render over the canvas.
4. `cl_hud_loaded` fires once on overlay mount (with `realm_name`,
   `comms_protocol`).

The primary metric `cl_hud_loaded_rate` is derivable from the single
`cl_hud_loaded` event per session. Live peer presence, online-friend dots and
the notifications feed are NOT browser-reachable from the public catalyst edge --
they arrive via the engine bridge (`window.dclBridge`); absent it, the HUD shows
ui3 static/empty state.


## Browser play data (2026-09-18)

The deployed browser overlay now consumes `/api/screens/v1/play`: one streamed
public-data bundle for featured/browse places, live events, wearables, emotes
and outfits. Existing query keys receive each section as it arrives. Native
and editor overlays retain their direct loaders. Private notifications/social
reads keep their existing signed paths; engine and scene assets remain separate.

Mounted-hook tests verify one shared request, cache reuse and invalidation;
stream tests verify early consumption and rejection of incomplete responses.
A production-build handler test confirms early frames while inventory is held.
This is data-path verification, not a live latency or HUD conversion measurement.
