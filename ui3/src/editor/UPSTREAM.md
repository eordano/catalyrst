# Creator Hub / Bevy integration alignment

Reviewed 2026-09-18 against the local `github.com-decentraland/creator-hub`
mirror at `47a0b5a97ef9a68339a3504f99030e07d7c7852f` (2026-09-17).
The ribbon remains the product's navigation surface. These changes align
integration behavior; this is not a wholesale replacement with upstream's
Inspector/ECS implementation.

Upstream references:

- `packages/inspector/src/lib/renderer/bevy/engine-iframe.ts`: same-origin
  engine console readiness, explicit reload, and rebinding window-owned hooks.
- `packages/inspector/src/lib/renderer/bevy/input-focus-bridge.ts`: disposable
  key forwarding, bubbling through body/document, numeric key codes, viewport
  focus, and keeping unmodified input in the scene while interacting.
- `packages/inspector/src/lib/renderer/bevy/scene-run-bridge.ts`: completion
  acknowledgement before leaving a reset transition.
- `packages/inspector/agents/bevy/src/camera.ts`: virtual-camera ownership and
  avatar input follow the camera mode; freezing a scene preserves native navigation.
- `packages/inspector/src/lib/renderer/bevy/selection-bridge.ts`: synchronizing
  current selection and component state with the renderer.
- `packages/creator-hub/renderer/src/hooks/useEditor.ts`: project persistence
  and runtime state must describe the same project.

## Applied here

- A viewport navigation/retry owns a fresh editor-bus session. Selection,
  inspector values, transform cache and undo history are reset together. A
  module-global set of composite strings no longer suppresses hydration in a
  later editor or after reconnecting.
- Engine console availability is recognized, but an editing workspace only
  becomes usable after the scene handshake and saved-scene restoration reply.
  An agent with no scene is not an editable workspace. A crash or failed restore
  produces a retryable error and disables live ribbon mutations.
- The existing generic RPC transport now exposes `restoreComposite` on the
  editor agent. Initial hydration reconciles the saved scene rather than adding
  a second copy. Invalid composites and failed writes/imports reject. Agent
  restoration resumes a paused scene before applying writes.
- Initial selection includes inspector values; later component changes on the
  same selected entity also produce updates. Selection ids alone are not a
  sufficient change detector.
- Input listeners are disposed and rebound per viewport session. Forwarded
  events bubble from the host body, carry legacy numeric key codes, and retain
  modifier shortcuts. Bare keys remain in the scene during Play. Existing ribbon
  tool shortcuts and camera gestures remain intentional local affordances.
- Play requires a preserved authored snapshot. Stop waits for restoration to
  succeed before reporting completion. Failed transitions remain visible and
  retryable; responses from a previous viewport cannot change a new session.
- Play/Pause/Resume/Stop now use acknowledged agent RPCs, selecting the project
  before issuing scene commands. The host no longer fires unacknowledged freeze
  messages at the iframe. Pause also freezes rendered animation; it preserves
  native avatar navigation, matching upstream. The ribbon exposes Pause/Resume
  next to Stop; F5 resumes a paused preview without reloading the page.
- Play releases the virtual camera and avatar input modifier, removes the
  editor's avatar-hiding area, and suspends picking, gizmos and editor mouse
  gestures. Stop restores the editor camera mode, position, orientation and
  projection. Orbit tool keys no longer move the camera unless pointer-locked.
- Delete now calls the engine's `delete_entity` command before removing the
  agent snapshot entry. Previously the bus handler only changed the snapshot.
  Missing engine API methods reject instead of returning a successful no-op.
- Publishing and disk saving share the same authoritative engine-composite
  reader. A failed live export cannot silently publish the original seed.
- Focusing the viewport from a ribbon control no longer cancels an active
  camera drag. The agent waits for the project scene during startup, and its
  camera readout follows the native camera during Play and Pause.
- Editor and preview launch URLs explicitly load the hosted character controller.
  The public origin follows the gateway's forwarded host and protocol; browser
  movement no longer depends on an unset WASM world-service endpoint.

## Verification and limits

Focused UI tests exercise reconnect, duplicate ready announcements, equal
composites in separate mounts, missing scenes, restoration failure, failed Play
snapshots, acknowledged playback transitions, engine failures, input routing,
root protection, and boot transitions. `npm test` in `bevy-explorer/editor-scene`
checks command sequencing and uses real SDK components to check camera/input
release, paused navigation and editor-pose restoration. Existing disk
save tests cover invalid/missing exports. Chromium stories use a clearly marked
simulated engine iframe to exercise the real BroadcastChannel + reload boundary;
including Play/Pause/Resume/Stop; they do not exercise GPU rendering or the actual
Bevy scene runtime. Storybook:
`Editor/Pages/Workspace bridge` (Connected / RestoreFailed).

The standing hardware verifier is `tools/screen-tour/verify-editor-playback.mjs`.
It opens an isolated Chromium context through CDP and exercises the actual Bevy
iframe with mouse and keyboard input. Use `CDP=http://127.0.0.1:9222 node
tools/screen-tour/verify-editor-playback.mjs` from the monorepo root. Optional
`SITES_ORIGIN` routes the document/assets to a local build; `EDITOR_EVIDENCE`
selects the screenshot, console-log and result directory.

On 2026-09-18 both the local Nix package and deployed catalyst.example.com passed hardware
WebGPU validation on NVIDIA Lovelace: editor orbit, Play ticks, character
movement, native camera look during Play and Pause, frozen scene ticks, Resume,
and exact editor-camera restoration on Stop. Live evidence is in
`/tmp/editor-deployed-validation/result.json` and its adjacent screenshots.
The live run moved the avatar 4.23 metres and held the paused tick at 166.

The sites profile entry and editor-agent service were redeployed together.
Sites payload: `/nix/store/667mdxz72vdmxd4jmhmngfdg35dqdjgl-sites-0.0.0`.
A previously built editor agent does not expose `restoreComposite`, `setPlayback`
or `stepPlayback`. The agent service builds with `dcl-one-sdk start`; restart it
after a separate SDK-commands build so its split-runtime loader is regenerated.

Remaining architectural differences: upstream owns an inspector ECS and forwards
edits through its renderer abstraction; this implementation owns state in the
editor scene and uses the generated `type`-based bus. It is not wire-compatible
with upstream's `kind`-based inspector-agent protocol. Upstream Stop also resets
the scene runtime; our acknowledged composite restore does not restart arbitrary
user-script timers or module state. Project-realm caching and cross-tab bus
isolation need a separate migration before claiming complete parity. The GPU
checks use a fresh temporary project; disk save/reopen is covered by unit tests,
not by this hardware playback verifier.
