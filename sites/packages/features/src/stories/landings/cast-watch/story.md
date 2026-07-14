---
id: landings-cast-watch
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Resolving a Cast 2.0 watch link to a clear, recoverable join surface -- an
    onboarding card that names the stream and offers a one-tap JOIN NOW when a
    stream is live, and an honest "no one is currently casting" waiting state
    when it is not -- increases the share of opened /cast/w/:location links that
    reach the live watch view, even with the LiveKit subscribe simulated.
  because: >-
    A watcher follows a share link minted on-demand by the in-scene Admin Smart
    Item, so by the time they arrive the stream may be live, not started yet, or
    already ended. Today an unresolved link is an opaque dead end; naming the
    place, showing a Viewer tag + audio-output selector, and distinguishing the
    "no active stream" waiting state (recoverable -- it may start soon) from an
    expired-access error means more watchers who open a valid link reach LIVE
    instead of bouncing, and the ones who cannot are told why.
metric:
  primary: cast_watch_join_rate
  numerator: cast_watch_joined
  denominator: cast_watch_opened
  guardrails:
    - cast_watch_opened
    - cast_watch_no_stream
    - cast_watch_access_expired
decision:
  rule: >-
    This is a spec-priority surface: a single shipping variant resolves the watch
    link and instruments the join funnel. Read out cast_watch_join_rate =
    cast_watch_joined / cast_watch_opened, and watch the guardrails -- the
    no-active-stream waiting path (cast_watch_no_stream) and the expired-access
    path (cast_watch_access_expired) must stay graceful and recoverable. Ship the
    guided onboarding if the join rate clears the MDE with no guardrail
    regression; otherwise hold.
experiment:
  key: st_cast_watch
  unit: session
  variants:
    - id: watcher
      weight: 1
      flags:
        guidedWatch: true
  baseline: 0.5
  mde: 0.05
  min_sample: 3000
---

# Cast: join & watch a live stream (watcher)

The Cast 2.0 watcher surface (`/cast/w/:location`, ui3 `StCastWatcher`) lets a
viewer watch a live broadcast from a Decentraland scene or World. `location` is a
parcel coordinate (e.g. `0,0`) or a World name; a watch link is minted on-demand
by the Admin Smart Item in a scene and shared. Opening it lands the viewer on this
surface, which resolves the location to a room, mints a LiveKit viewer credential,
and either drops them into onboarding (a live stream exists) or shows the waiting
empty-state (no one is casting yet).

This story walks that join as a loader-driven, URL-addressable flow and tracks
whether the guided onboarding increases the share of opened watch links that reach
the live watch view.

- **Primary metric:** `cast_watch_join_rate` = `cast_watch_joined` / `cast_watch_opened`.
- **Guardrails:** the watch-opened volume (`cast_watch_opened`), the no-active-stream
  waiting path (`cast_watch_no_stream`), and the expired-access path
  (`cast_watch_access_expired`) must stay healthy / graceful.

## URL (deep-link addressable via the loader)

`/landings/cast-watch?location=<parcel|world>&name=<identity>&active=<1|0|expired>`

- `location` -- the watch surface to resolve (parcel coords like `0,0`, or a World
  name). Defaults to `0,0` (Genesis Plaza). A `missing` link (no resolvable
  surface) hands off to `cast-not-found` as a 404.
- `name` -- the viewer's display identity (defaults to `Viewer`).
- `active` -- which simulated upstream state to resolve:
  - `1` (default) -- a live stream is present -> onboarding -> JOIN NOW -> live.
  - `0` -- no active stream -> the waiting empty-state (NoActiveStreamError).
  - `expired` -- the stream access TTL passed -> the expired-access notice
    (ExpiredStreamAccessError).

## Journey steps

1. `open` -- resolve the watch surface for `?location` (simulated watcher-token +
   stream-info). active stream -> onboarding; no active stream -> waiting; missing /
   invalid link -> hand off to `cast-not-found`.
2. `onboarding` -- "Decentraland Cast from {streamName}", the Viewer tag, an
   audio-output selector, and JOIN NOW.
3. `joining` -- a transient spinner while the simulated subscribe resolves.
4. `live` -- the watch stage: the LIVE pill, the video frame (stub), an optional
   In-World Chat sidebar, and the Leave / mute / chat / people controls.
5. `waiting` -- the EmptyStreamState: "No one is currently casting to this link..."
   (the stream may be starting soon or has already ended).

## Emitted events

- `experiment_exposed` -- on the watch surface rendering (exposure attribution,
  emitted by the loader via `trackExposure`).
- `cast_watch_opened { location, is_world }` -- the watcher surface mounted / the
  token resolved. The funnel **denominator**. Emitted server-side so it is
  attributed even with JS disabled.
- `cast_watch_no_stream { location }` -- NoActiveStreamError / the waiting
  empty-state (no one is casting; recoverable -- "starting soon or already ended").
- `cast_watch_joined { room_id, place_name, stub: true }` -- JOIN NOW -> the live
  watch view reached. The funnel **numerator**. The LiveKit subscribe is SIMULATED.
- `cast_watch_chat_opened` -- the In-World Chat sidebar was toggled open (watch
  engagement).
- `cast_watch_muted` / `cast_watch_unmuted` -- the tab-audio mute toggled while
  in-World.
- `cast_watch_left` -- Leave pressed (left the watch room).
- `cast_watch_access_expired { location }` -- the ExpiredStreamAccessError path
  (graceful).

`cast_watch_join_rate = cast_watch_joined / cast_watch_opened`.

## Data reality (SIMULATED)

There is no watcher-token mint on the live catalyst (`catalyst.example.com`): the
`comms-gatekeeper` service (which issues `POST /cast/watcher-token` ->
`{ url, token, roomId, identity, placeName? }` and `GET /cast/stream-info/:streamingKey`)
is not deployed there, and the catalyst comms crate handles WebRTC RPC, not browser
cast. So token resolution + the LiveKit subscribe (rendering the remote video
track) are **simulated** by `lib/catalyst/cast-watcher.server` (getJSON + zod with a
fixture fallback) against the faithful upstream shapes in
`app/fixtures/cast-watcher.json` (sourced from decentraland/comms-gatekeeper main --
watcher-token-handler.ts, logic/cast/types.ts `GenerateWatcherCredentialsResult`,
errors.ts) and verbatim ui3 `StCastWatcher` copy. The flow, states, recoverable
error paths, and telemetry are real; the final LiveKit subscribe is a clearly-noted
stub. There is **no XState machine** -- this is a spec-priority, loader-driven
surface (sibling to the `landings-cast-stream` broadcaster console).
