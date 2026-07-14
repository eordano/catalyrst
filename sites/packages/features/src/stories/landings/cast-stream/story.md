---
id: landings-cast-stream
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided broadcaster console (resolve token -> select devices -> grant
    permissions -> preview -> go live) increases the share of opened cast links
    that reach a LIVE broadcast, even with the LiveKit room join simulated.
  because: >-
    Casting into a scene/World fails silently when a device or permission is
    wrong; making each pre-flight step explicit and recoverable (re-pick a
    device, retry permissions, see a preview before going live) means more
    streamers who open a valid /cast/s/:token link reach LIVE instead of
    abandoning at an opaque "join" button.
metric:
  primary: cast_go_live_rate
  numerator: cast_went_live
  denominator: cast_token_checked
  guardrails:
    - cast_token_checked
    - cast_permissions_denied
    - cast_invalid_token
decision:
  rule: >-
    Ship if cast_go_live_rate improves by at least the MDE with no guardrail
    regression (token-check volume holds, the permission-denied and invalid-link
    paths stay graceful and recoverable); otherwise hold.
experiment:
  key: st_cast_console
  unit: session
  variants:
    - id: console
      weight: 1
      flags:
        guidedConsole: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
---

# Cast / stream into a scene or World (LiveKit broadcaster console)

> Canonical story name: **cast-streamer-go-live** (CAST persona -- streamer / scene
> operator going live). This is the existing `landings-cast-stream` story; the
> directory, route, fixture, and telemetry `story` id stay `landings-cast-stream`
> for stability, with `cast-streamer-go-live` as the canonical human name.

The Cast 2.0 broadcaster console (`/cast/s/:token`, ui3 `StCastStreamer`) lets a
streamer broadcast mic/camera/screen into a Decentraland scene or World. A stream
link is minted on-demand by the Admin Smart Item in a scene; opening it lands the
streamer on this console, which must resolve the token, set up devices, grant
browser media permissions, preview, and then go LIVE.

This story walks that pre-flight as an explicit, recoverable wizard and tracks
whether the guided flow increases the share of opened cast links that reach LIVE.

- **Primary metric:** `cast_go_live_rate` = `cast_went_live` / `cast_token_checked`.
- **Guardrails:** token-check volume (`cast_token_checked`), the permission-denied
  path (`cast_permissions_denied`), and the invalid-link path (`cast_invalid_token`)
  must stay healthy / graceful.

## Journey steps (URL-addressable via `?step=`)

1. `token-check` -- validate the `:token` (streamingKey) by POSTing it to
   comms-gatekeeper `/cast/streamer-token`. SIMULATED here.
2. `device-select` -- pick mic / speaker / camera (the onboarding modal).
3. `permissions` -- request browser media permissions (getUserMedia). SIMULATED.
4. `preview` -- onboarding modal with a self-preview; confirm and JOIN NOW.
5. `live` -- the broadcasting view (LIVE pill, controls bar). LiveKit publish SIMULATED.
6. `ending` -- leave-stream confirmation / teardown in progress.
7. `ended` -- the cast has ended; offer to start a new one.
8. `invalid` -- the token was missing / malformed / expired (StCastNotFound).

## Emitted events

- `experiment_exposed` -- on console mount (exposure attribution).
- `cast_token_checked` -- entering `token-check` (the funnel denominator).
- `cast_token_valid` (`{ place_name, is_world }`) -- token resolved OK.
- `cast_invalid_token` (`{ reason }`) -- token missing/expired/invalid -> `invalid`.
- `cast_devices_selected` (`{ mic, speaker, camera }`) -- devices chosen.
- `cast_permissions_granted` / `cast_permissions_denied` -- getUserMedia outcome.
- `cast_preview_ready` -- preview shown, ready to join.
- `cast_join_requested` -- JOIN NOW pressed (room-join requested).
- `cast_went_live` (`{ room_id, stub: true }`) -- LIVE reached (the numerator).
- `cast_screenshare_started` (`{ room_id, stub: true }`) -- screen-share track
  published in the live view (LiveKit publish SIMULATED).
- `cast_screenshare_failed` (`{ room_id, stub: true }`) -- screen-share publish
  failed (mirrors the "Screen sharing failed" toast; recoverable retry).
- `cast_ending` -- LEAVE STREAM pressed (teardown started).
- `cast_ended` (`{ stub: true }`) -- room closed, devices released.

## Data reality (SIMULATED)

There is no streaming-token mint on the live catalyst (`catalyst.example.com`): the
`comms-gatekeeper` service (which issues `POST /cast/streamer-token` ->
`{ url, token, roomId, identity }`) is not deployed there, and the catalyst comms
crate handles WebRTC RPC, not browser cast. So token resolution + the LiveKit
room join (publishing tracks) are **simulated** via an XState machine against the
faithful upstream shapes in `app/fixtures/landings-cast-stream.json` (sourced from
decentraland/comms-gatekeeper). The flow, states, recoverable error paths, and
telemetry are real; the final on-room commit (LiveKit publish) is a clearly-noted
stub.
