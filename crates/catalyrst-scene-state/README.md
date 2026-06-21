# catalyrst-scene-state

Rust port of [`decentraland/scene-state-server`]. Hosts authoritative, server-side state for SDK7 multiplayer scenes: an HTTP control surface plus a per-scene WebSocket transport carrying CRDT state sync. Listens on **port 5209** by default (override with `HTTP_SERVER_PORT`).

[`decentraland/scene-state-server`]: https://github.com/decentraland/scene-state-server

Not a dumb CRDT relay: it runs the scene's own compiled SDK7 JavaScript (`bin/game.js`) headlessly inside V8 - a sandboxed `onStart` + 30 Hz `onUpdate` game loop where the scene declares its entity-range policy via `registerScene(...)` and authoritatively drives multiplayer CRDT state.

## Architecture (upstream -> this crate)

| upstream file                              | this crate                       |
|--------------------------------------------|----------------------------------|
| `logic/protocol.ts`                        | `protocol.rs`                    |
| `controllers/handlers/ws-handler.ts`       | `ws.rs` + `auth.rs`              |
| `adapters/scene.ts`                        | `runtime.rs` + `scene.rs`        |
| `adapters/wsRegistry.ts`                   | `scene.rs::SceneManager`         |
| `logic/sceneFetcher.ts`                    | `scene_fetcher.rs`               |
| `logic/scene-runtime/{sandbox,sdk7-runtime,apis}.ts` | **`jsruntime.rs`**     |
| SDK7 CRDT wire format + LWW merge (`@dcl/ecs`) | **`crdt.rs`**                |

### `crdt.rs` - the SDK7 CRDT engine

Pure-Rust port of the `@dcl/ecs` CRDT wire format and LWW-element-set merge:

- Wire codec (little-endian, matching `@dcl/ecs`'s `ByteBuffer`): the 8-byte `[length u32, type u32]` header plus per-type bodies for `PUT_COMPONENT`, `DELETE_COMPONENT`, `DELETE_ENTITY` and `APPEND_VALUE`. Unknown message types are skipped by length; truncated trailers are ignored.
- LWW merge keyed by `(entity, componentId)`: highest Lamport `timestamp` wins; on a tie the lexicographically-greater data wins (length first, then bytes - the `dataCompare` rule), with `DELETE_COMPONENT` modelled as `data = null` sorting below any present data. `DELETE_ENTITY` tombstones the entity (drops its components and masks every later op for it).
- Snapshot serialization (the late-joiner `Init` state) and per-client range reclaim (`reclaim_range`) for `on_client_close`.

### `jsruntime.rs` - the server-side JS runtime

Embeds V8 (via the [`v8`] crate - i.e. `rusty_v8`, the same engine `deno_core` wraps) on a dedicated per-scene OS thread (a V8 isolate is single-threaded). The async server talks to it over an MPSC `Command` channel plus a mutex-guarded `SharedState` the WS tasks read synchronously for the `Init` frame. Reproduces the upstream sandbox:

- Globals (`sandbox.ts` / `sdk7-runtime.ts`): `module`/`exports`, `console`, `require('~system/*')`, `setImmediate`, `registerScene`, `updateCRDTState`, and restricted `fetch`/`WebSocket` (both throw).
- `~system/*` host modules (`apis.ts`): `EngineApi.crdtSendToRenderer({data}) -> {data:[]}` (merges the scene's CRDT batch into the authoritative engine and refreshes the snapshot); `EngineApi.crdtGetState() -> {hasEntities, data:[Uint8Array]}`; `EngineApi.isServer() -> {isServer:true}`; `EngineApi.sendBatch() -> {events:[]}`; `Runtime.getRealm()`, `Runtime.getSceneInformation()`, `Runtime.readFile()`; `UserIdentity.getUserData() -> {}` and `SignedFetch.getHeaders() -> {}` (no-ops, exactly as upstream).
- Game loop: `onStart` then a 30 Hz `onUpdate` (first tick `dt = 0.0`), draining `setImmediate` and pumping microtasks each tick.
- Multiplayer wiring: `registerScene(config, observer)` captures the entity-range policy and the client observer; each connected client is surfaced to the scene as `{ sendCrdtMessage(bytes), getMessages() }`, and the scene pulls inbound client batches and pushes merged output from inside its own `onUpdate`, as upstream.

`RelayRuntime` (also in `runtime.rs`) is the scene-logic-free fallback for scenes with no `game.js` or when `DISABLE_JS_RUNTIME=1` - backed by the same real CRDT engine (deduplicated snapshot, real LWW merge, range reclaim).

## V8 under Nix (offline build) - IMPORTANT

The `v8`/`rusty_v8` crate's `build.rs` normally downloads a prebuilt `librusty_v8_release_<target>.a` from GitHub, which fails inside the Nix sandbox (no network). The fix - the same one nixpkgs uses for `deno`, `codex`, `windmill`, etc. - is to fetch that archive as a fixed-output derivation (pinned by hash, network allowed) and hand its path to the crate via the `RUSTY_V8_ARCHIVE` env var; the build then links the prebuilt static V8. Already wired in the workspace `flake.nix`:

```nix
librusty_v8 = pkgs.callPackage ./crates/catalyrst-scene-state/nix/librusty_v8.nix { };
catalyrst-scene-state = pkgs.rustPlatform.buildRustPackage {
  # ... cargoLock, cargoBuildFlags ...
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.openssl ];
  env = {
    OPENSSL_NO_VENDOR = "1";
    RUSTY_V8_ARCHIVE = "${librusty_v8}";   # <-- the one var that matters
  };
};
```

`nix/librusty_v8.nix` is a `fetchurl` of `librusty_v8_release_x86_64-unknown-linux-gnu.a.gz` from the rusty_v8 v149.3.0 release (gunzipped into a single-file store path). The version must match the `v8` crate pin in `Cargo.toml` (`v8 = "=149.3.0"`); bump both together and refresh the hash with:

```sh
nix-prefetch-url \
  "https://github.com/denoland/rusty_v8/releases/download/v<VER>/librusty_v8_release_x86_64-unknown-linux-gnu.a.gz" \
  | xargs nix hash to-sri --type sha256
```

No from-source V8 build, no `gn`/`ninja`, no debian sysroot download - just the ~37 MB prebuilt archive. (nixpkgs also offers a from-source `rusty-v8` derivation if a prebuilt archive is ever unavailable for a target; the prebuilt path is what `deno` itself ships with.)

[`v8`]: https://crates.io/crates/v8

## Configuration

| env var              | meaning                                                        |
|----------------------|----------------------------------------------------------------|
| `HTTP_SERVER_HOST/PORT` | bind address (default `127.0.0.1:5209`) |
| `LOCAL_SCENE_PATH`   | local compiled `game.js` to load as `localScene` at startup    |
| `WORLD_SERVER_URL`   | worlds content-server for `/debugging/reload <world>`          |
| `DEBUGGING_SECRET`   | shared secret gating `POST /debugging/reload`                  |
| `REALM_NAME`         | realm reported via `~system/Runtime.getRealm()` (def `dcl-one`) |
| `DISABLE_JS_RUNTIME` | `1` to use the scene-logic-free `RelayRuntime` instead of V8   |
| `AUTH_TIMEOUT_SECS`  | seconds to wait for the WS `Auth` frame (default 5)            |

## Tests

```sh
# the v8 crate's first build downloads the V8 archive, so the test command must
# run in an environment with network access (or with RUSTY_V8_ARCHIVE preset)
cargo test -p catalyrst-scene-state
```

Covers the CRDT codec + LWW merge (`crdt.rs`), the relay merge/snapshot path (`runtime.rs`), and three V8 integration tests running real JavaScript: an `onStart` write via `EngineApi.crdtSendToRenderer`; client-message relay through the `registerScene` observer + `getMessages`/`sendCrdtMessage`; and entity-range reclaim on client close.
