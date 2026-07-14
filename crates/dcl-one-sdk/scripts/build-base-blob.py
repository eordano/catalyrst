#!/usr/bin/env python3
"""Build `src/vendor/node_modules.zip` — the offline scene toolchain.

`init` extracts this blob so a scaffolded scene bundles, type-checks and
previews with no npm and no network. It is pure JS — no platform binaries — so
one blob serves linux, macOS and Windows.

The install list is *derived from the scaffold manifest*
(`src/templates/init/scene/package.json`), never hardcoded: the vendored set
has to match what a later `npm install` in a scaffolded scene would produce, or
the two fight instead of converging.

The blob does NOT ship the SDK as source. It ships the *build products* that
source was only ever used to derive:

* two prebuilt runtime chunks, `@dcl/sdk/prebuilt/{core,smart}.js`, and
* one rolled-up ambient declaration file, `@dcl/js-runtime/index.d.ts`.

**Why the chunks.** The SDK runtime chunk is scene-independent. Two scenes —
one importing three `@dcl/ecs` symbols, one importing react-ecs UI + tweens +
audio + animator + players + network — produce byte-identical
`bin/sdk-runtime.js`; it is keyed only on which `@dcl/*` packages are
installed, never on what the scene imports. So 3.64 MB of `.js` shipped for one
purpose: letting rolldown re-derive the same artifact on every build. It is
built here once instead, by `dcl-one-sdk vendor-chunks` (a hidden subcommand
over `src/prebuilt.rs`), which is the only place either chunk is ever produced.

The split into *two* chunks is what makes `@dcl/asset-packs` affordable in this
blob at all. `entrypoint.rs` inlines the smart-item script runtime whenever
`@dcl/asset-packs` merely *resolves*, which grew the single chunk from
463,133 B to 601,100 B (+137,967 B, +30%) — reproducibly, even for a scene with
no composite and no smart item anywhere. Vendoring asset-packs as source would
have made that inflation universal. Split, the smart-item runtime is its own
150 KB chunk, and `src/build.rs` installs it only when
`Project::is_editor_scene()` is true or the built scene chunk actually names
`@dcl/asset-packs` (`0,0-cube-spawner` in decentraland/sdk7-test-scenes imports
`@dcl/asset-packs/dist/scene-entrypoint` from its own source with no composite
at all, so the composite test alone is not enough).

Measured here: core 463,815 B, smart 150,017 B. A smart-item scene ships
613,832 B across two chunks where it used to ship 603,566 B in one (+1.7%);
every scene *without* smart items ships 463,815 B instead of 603,566 B
(−139,751 B) — and neither chunk is rebuilt on any scene build.

Two registry keys had to be added for the split: `@dcl/sdk/platform` and
`@dcl/sdk/text-codec`. `@dcl/asset-packs` requires both, and once it lives in
its own chunk those requires cross a chunk boundary. Confirmed by
`verify_requires()` in `src/prebuilt.rs`: the smart chunk's complete external
require set is `@dcl/ecs`, `@dcl/ecs-math`, `@dcl/ecs/dist/components`,
`@dcl/react-ecs`, `@dcl/sdk/{ecs,math,message-bus,platform,text-codec}` and six
`~system/*`. Adding them cost the core chunk 682 B.

**Why the rolled-up types.** `build --production` type-checks the scene, so the
declarations have to be there even though the implementations do not. Rolling
`@dcl/{sdk,ecs,react-ecs,ecs-math,js-runtime,asset-packs}` plus the
`protobufjs`/`long`/`mitt` declarations they reach into ONE ambient `.d.ts`
replaces those `.d.ts` trees and, more to the point, lets the `.js` beside them
go. See `build_types_rollup()` for the six transforms this forces and why each
is mandatory. Verified: all 60 sdk7-test-scenes type-check to **byte-identical
diagnostics** under the rolled-up types and under their own `node_modules` (55
clean under both, 5 failing identically for unrelated missing third-party
deps).

An ambient `declare module "X"` wins over a real `node_modules/X` whose `types`
entry does not resolve — which is what lets `@dcl/ecs` stay on disk as a
runtime-only `dist-cjs` tree (the node-side crdt dumper needs it) while its
types come from the rollup. Checked directly: a scene importing `@dcl/ecs`,
`@dcl/sdk`, `@dcl/sdk/math` and `@dcl/asset-packs/dist/scene-entrypoint`
type-checks clean against exactly that tree.

What gets dropped, and the evidence for each:

* The `.js` *and* `.d.ts` of `@dcl/sdk`, `@dcl/react-ecs`, `@dcl/ecs-math`,
  `react`, `react-reconciler`, `scheduler` and `@dcl/ecs/dist` — 4.55 MB
  in total. Every byte is already inside the two prebuilt chunks (runtime) or
  the rolled-up declaration file (types). `@dcl/sdk` keeps only its manifest
  and `types/*.json`: every scene's tsconfig extends
  `@dcl/sdk/types/tsconfig.ecs7.json`, and rolldown reads it too.
* `@dcl/ecs/dist-cjs` does NOT go. `data_layer.rs` regenerates `main.crdt` by
  running `templates/data-layer-host.mjs` in node, which loads the
  `@dcl/inspector` stand-in below, which requires `@dcl/ecs/dist-cjs` and
  `@dcl/ecs/dist-cjs/serialization/ByteBuffer` — the CRDT wire codec, in node,
  outside any chunk. That is also why `protobufjs/minimal`, `long` and the
  `@protobufjs/*` micro-packages stay: `dist-cjs` imports them.
* `@dcl/asset-packs` entirely (34 MB installed; 1.3 MB even after the `bin/`
  prune `inspector.zip` applies). Its runtime is in the smart chunk and its
  declarations are in the rollup, so nothing reads the package itself. That is
  what "put `@dcl/asset-packs` in the base blob" costs here: 150 KB of chunk
  and 103 KB of declaration rollup, not 1.3 MB of source.
* The `.d.ts` of `protobufjs` and `long` (0.10 MB). They were kept because
  `@dcl/ecs/dist/*.gen.d.ts` imported `protobufjs/minimal` and tsc followed;
  with `@dcl/ecs/dist` gone nothing type-references them, and the 60-scene
  corpus check above ran with both packages absent from the type tree.
* `@dcl/rpc` down to its runtime closure (66 files / 0.31 MB -> 14 files /
  0.11 MB) and `mitt` down to `dist/mitt.js` (10 files / 0.03 MB -> 3 files /
  0.004 MB). See `RPC_RUNTIME` below for the walk. `mitt` used to be dropped
  outright; it comes back because `@dcl/rpc` requires it in node, outside every
  chunk — but only the runtime file, so the rollup keeps owning its types.

**Why `@dcl/rpc` is in the blob at all.** The visual editor talks to a
*data-layer host* over a WebSocket: a 22-method RPC service carrying the
engine's CRDT stream in both directions. Upstream's host is `@dcl/inspector`'s,
and pulling that back in costs 119 MB for a package that is ~90% browser UI. So
the blob ships its own: `src/vendor/inspector-shim` grew from a single
`dumpEngineToCrdtCommands` into a minimal host — 4 of the 22 methods live
(`crdtStream`, `save`, `get`/`setInspectorPreferences`), the other 18 inert but
well-formed. It is a FALLBACK, not a takeover: `req()` in
`templates/data-layer-host.mjs` resolves the scene's own `node_modules` first,
so an installed `@dcl/inspector` still wins.

What that needed, and only that: `@dcl/rpc` (the wire protocol —
port multiplexing, framing, bidirectional streams; vendored, not
reimplemented), `mitt`, and the generated service descriptor
(`build_service_descriptor()`). The expensive half was already here: `@dcl/ecs`'s
live engine and CRDT reconciliation ship in `dist-cjs` for the crdt dumper, and
`protobufjs/minimal` + `long` ship for `dist-cjs`. Net cost ~0.28 MB unpacked.
`@well-known-components/pushable-channel` is NOT needed even though upstream's
`stream.ts` imports `AsyncQueue` from it — `@dcl/rpc/dist/push-channel` already
exports an `AsyncQueue` of the same shape.

What gets dropped, and the evidence for each (pre-existing prunes):

* `typescript/lib/typescript.js` (8.7 MB), `typescript.d.ts` (0.6 MB),
  `_tsserver.js`, `tsserverlibrary.*`, `typingsInstaller.js`, `watchGuard.js`,
  `typesMap.json`. `build.rs` runs exactly one thing —
  `node typescript/lib/tsc.js -p tsconfig.json --noEmit` — and a run of that
  under a node with `Module._resolveFilename` and the `fs` read family hooked
  resolves precisely two files inside the package: `lib/tsc.js` and the
  `lib/_tsc.js` it requires. `typescript.js` is the programmatic API, the
  `tsserver*` files are the editor language service; nothing in this crate,
  in `data-layer-host.mjs`, or in a scene loads either.
* `typescript/lib/<locale>/diagnosticMessages.generated.json` — 13 locales,
  4.2 MB. tsc reads at most one, only when `--locale`/the host locale asks for
  it, and `localizedDiagnosticMessages` stays undefined when the file is
  absent: messages come out in English instead of failing.
* The whole of `ethers` (10.7 MB) and the four packages only it reached
  (`@adraffy/ens-normalize`, `aes-js`, `@noble/curves`, `@noble/hashes`).
  `src/vendor/README.md` used to call ethers "imported by
  `@dcl/sdk/ethereum-provider` but undeclared in its manifest". It is not
  imported. A specifier scan of every `.js`/`.ts` in the blob outside
  `ethers/` itself returns three hits, and all three are the same line of a
  `/** ... */` doc comment:

      node_modules/@dcl/sdk/ethereum-provider/index.js:8
      node_modules/@dcl/sdk/ethereum-provider/index.d.ts:8
      node_modules/@dcl/sdk/src/ethereum-provider/index.ts:8
          * import { ethers } from 'ethers'

  `@dcl/sdk`'s manifest does not declare `ethers`, and neither does the
  scaffold's `package.json` — so `npm install` in a scaffolded scene does not
  install it either. Shipping it was the divergence, not dropping it. A scene
  that wants ethers must add it to its own `package.json`, exactly as it must
  in the npm flow. To put it back, add `'ethers'` to `EXTRA_INSTALL` and
  `ENTRY_PACKAGES` below.
* `protobufjs` — ALL of it, plus six of the seven `@protobufjs/*`
  micro-packages. `add_pbmin()` writes `node_modules/protobufjs` from
  `experiments/protobufjs-minimal-replacement` instead: a dependency-free
  reimplementation of the `Reader`/`Writer`/`util.Long`/`configure` surface,
  4 files / ~48 KB where upstream's minimal closure plus its micro-packages
  were 38 files / 99,989 B. `@protobufjs/utf8` is the one that stays —
  `@dcl/ecs/dist-cjs` requires it *directly*, not through protobufjs, so the
  replacement does not subsume it. `long` stays too: `avatar_shape.gen.js` and
  `descriptor.gen.js` import it by name.

  This is a hand-written codec on the critical path of the scene protocol, so
  the bar for it is a differential suite, not a review. See `add_pbmin()` for
  the numbers; the short version is 731,200 differential message instances and
  800,000 fuzz operations per seed against upstream 7.2.4 as the reference,
  across `@dcl/ecs` (both the CJS and the ESM build), `@dcl/rpc`'s framing and
  the data-layer descriptor, asserting byte-identical encodings and
  throw-parity — zero divergences over six seeds, and 13/13 on injected
  wire-format mutants.

  This DOES change the scene runtime. `swap_pbmin_into_tree()` redirects the
  install tree's `minimal.js` before `build_chunks()` resolves it, so
  `prebuilt/core.js` bundles the replacement and QuickJS runs it too.
  `check_chunk_pbmin()` fails the build if it ever stops doing so.

  It did not always. The swap first landed node-side only — `@dcl/ecs/dist-cjs`
  (69 files), `@dcl/rpc` (6), `@dcl/inspector/data-layer.gen.js` (1) — which
  left an extracted scene carrying TWO codecs: the replacement in
  `node_modules`, upstream's minified inside the chunk. That is the state this
  removes, and it is the reason to care; the 25,873 B was never the point.

  Be clear about the cost, because it is real: a hand-written codec on the
  scene protocol's critical path fails in every scene, not in one dev-machine
  process. The differential suite below is what that risk is priced against,
  and it covers `@dcl/ecs` in both its CJS and ESM builds — which is what the
  chunk bundles — not just the node path.

  Same class of divergence as the `typescript` prune below, and one step
  further: a scene that imports `protobufjs` *itself* (the reflection library,
  or `protobufjs/light`) resolved under `npm install` and never offline, and
  now the offline `protobufjs` is not upstream's at all. It must declare
  protobufjs in its own manifest to import it, which the scaffold does not, so
  npm would give it its own full copy anyway — and the shipped manifest says
  `7.2.4-dcl-one-sdk-pbmin.1`, not `7.2.4`, so `npm ls` cannot mistake one for
  the other.
* `@types/node` (2.3 MB) and `undici-types` (0.1 MB). The scaffold's tsconfig
  inherits `"types": ["@dcl/js-runtime"]` from `@dcl/sdk/types/tsconfig.ecs7.json`,
  which turns off automatic `@types` inclusion, and the traced tsc run never
  opens a file under `@types/`. Nothing in the kept declaration closure carries
  a `/// <reference types="node" />` either (the resolver check below would
  fail if it did).
* Source maps, `.md`, `test/`, `docs/`, `example/`, `bench*/`, `.github/`:
  nothing loads them at runtime and tsc never reads them.

Two packages stay pinned *below* their latest release, deliberately:

* `protobufjs` 7.2.4, not 8.7.1. No upstream protobufjs runtime reaches the
  blob any more — not as a package (`add_pbmin()`) and no longer inside
  `prebuilt/core.js` either (`swap_pbmin_into_tree()`). What is still installed
  is its TYPES, which `build_types_rollup()` reads because `@dcl/ecs/dist`
  imports `protobufjs/minimal` without declaring it, and its runtime as the
  reference the differential suite compares against. The second is what keeps
  the version pinned: 7.2.4 is what the npm flow resolves, via `@dcl/sdk` ->
  `@dcl/sdk-commands` -> `@dcl/protocol`, whose manifest pins `"protobufjs":
  "7.2.4"` exactly. Installing 8.x would silently re-target the suite at a
  reference no scene runs, which is the one thing that would make the evidence
  behind the replacement worthless.
* `typescript` — see `TS_CEILING_NOTE` below and `src/vendor/README.md`.

Bootstrapping: the chunk build needs a `dcl-one-sdk` binary, and that binary
embeds this blob. There is no cycle — the binary that *builds* the chunks only
has to contain rolldown and `src/split.rs`, not the blob it will later ship. So
the order is: `cargo build --release` (with whatever blob is committed), run
this script, `cargo build --release` again to embed the new blob.

Usage:  python3 scripts/build-base-blob.py [--work DIR] [--keep-work]
                                           [--sdk-bin PATH]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.dirname(HERE)
WORKSPACE = os.path.dirname(os.path.dirname(CRATE))
SCAFFOLD = os.path.join(CRATE, 'src/templates/init/scene/package.json')
SHIM = os.path.join(CRATE, 'src/vendor/inspector-shim')
PBMIN = os.path.join(CRATE, 'experiments/protobufjs-minimal-replacement')
OUT = os.path.join(CRATE, 'src/vendor/node_modules.zip')
DEFAULT_SDK_BIN = os.path.join(WORKSPACE, 'target/release/dcl-one-sdk')
FIXED_DATE = (1980, 1, 1, 0, 0, 0)

# Where the prebuilt chunks land in the extracted tree. Inside the vendored
# `@dcl/sdk` on purpose: they are only valid for the `@dcl/sdk` version they
# were built from, and an `npm install` that replaces that package removes them
# in the same step, which flips a scene back to the from-source build path
# atomically instead of leaving a stale chunk behind. `src/prebuilt.rs` holds
# the same three paths.
PREBUILT_DIR = 'node_modules/@dcl/sdk/prebuilt'
CORE_CHUNK = f'{PREBUILT_DIR}/core.js'
SMART_CHUNK = f'{PREBUILT_DIR}/smart.js'
CHUNK_REGISTRY = f'{PREBUILT_DIR}/registry.json'
# The rolled-up ambient declarations replace @dcl/js-runtime's own index.d.ts,
# so tsconfig.ecs7.json's `"types": ["@dcl/js-runtime"]` keeps picking them up
# with no change to any scene's tsconfig.
TYPES_ROLLUP = 'node_modules/@dcl/js-runtime/index.d.ts'

TS_CEILING_NOTE = """typescript 7.x is the Go port: its `lib/getExePath.js` resolves a
per-platform native package (`@typescript/typescript-darwin-arm64` and friends),
which would make this blob platform-specific. 6.x is the ceiling until that is
addressed."""

# Installed on top of the scaffold's devDependencies. Every one of these is a
# real import that no manifest in the graph declares.
EXTRA_INSTALL = [
    # `@dcl/ecs/dist-cjs` imports 'protobufjs/minimal'; pinned to the version
    # the npm flow resolves (@dcl/protocol pins it exactly).
    'protobufjs@7.2.4',
    '@protobufjs/utf8',
    # `templates/data-layer-host.mjs` requires 'ws' for the data-layer socket.
    'ws',
    # The rpc runtime the visual editor's data layer speaks over that socket:
    # port multiplexing, request/response framing, bidirectional streaming.
    # Brings `mitt` (349 B of event emitter) with it as a real dependency.
    '@dcl/rpc',
    # Smart items. Never shipped as a package — `build_chunks()` bundles its
    # runtime into `prebuilt/smart.js` and `build_types_rollup()` folds its
    # declarations into the ambient bundle. Installing it here is what makes
    # both of those possible without a 12 MB `init --inspector`.
    '@dcl/asset-packs',
]

# The BFS roots. Only `@dcl/ecs` is a *runtime* root now, and only for its
# `dist-cjs`: everything else the scene needs at runtime is inside the prebuilt
# chunks, so the closure walk exists to keep the node-side crdt dumper and the
# type checker honest, not to assemble an SDK.
ENTRY_PACKAGES = ['@dcl/sdk', '@dcl/js-runtime', '@dcl/ecs', 'typescript',
                  'protobufjs', '@protobufjs/utf8', 'ws', '@dcl/rpc']

ENTRY_REASON = {
    '@dcl/sdk': 'types/tsconfig.ecs7.json + the prebuilt runtime chunks',
    '@dcl/js-runtime': 'scene ambient types (the rolled-up .d.ts)',
    '@dcl/ecs': 'dist-cjs: the crdt wire codec the inspector stand-in runs in node',
    'typescript': 'build --production type check',
    # Still walked, ships nothing: `add_pbmin()` writes the package instead.
    'protobufjs': "undeclared import of @dcl/ecs/dist-cjs ('protobufjs/minimal')"
                  ' - ships 0 upstream files, see add_pbmin()',
    # NOT subsumed by the replacement: `@dcl/ecs/dist-cjs` requires this by name.
    '@protobufjs/utf8': 'undeclared import of @dcl/ecs/dist-cjs',
    'ws': 'data-layer-host.mjs',
    '@dcl/rpc': 'the data-layer wire protocol (server, codegen, WebSocket transport)',
}

# npm-toolchain-only, ~160 MB, and only ever reached through `dependencies` —
# never through an import in the code that ships.
NEVER = {'@dcl/sdk-commands', '@dcl/explorer'}

# `protobufjs` is INSTALLED but nothing of it is SHIPPED — `add_pbmin()` writes
# `node_modules/protobufjs` instead, from
# `experiments/protobufjs-minimal-replacement`. See that function for the
# evidence. The package still has to be installed, but only for its TYPES:
# `build_types_rollup()` needs its declarations, because `@dcl/ecs/dist/*.gen.d.ts`
# import `protobufjs/minimal`. Its runtime is no longer used anywhere —
# `swap_pbmin_into_tree()` redirects `minimal.js` at the replacement before
# `build_chunks()` runs, so `prebuilt/core.js` bundles pbmin too and the whole
# toolchain has exactly one protobuf codec.
#
# Shipping nothing also has a second effect, and it is the one that pays: the
# BFS in `reachable()` walks only files `wanted()` keeps, so with protobufjs
# contributing no specifiers the six `@protobufjs/*` micro-packages it was the
# sole importer of fall out on their own — exactly the way
# `@protobufjs/{codegen,path,fetch}` already did. `@protobufjs/utf8` does NOT
# fall out and must not: `@dcl/ecs/dist-cjs` requires it directly
# (`serialization/ByteBuffer/index.js` and `components/component-number.js`),
# not through protobufjs, and it stays an entry point below.
PROTOBUFJS_SHIP_NOTHING = frozenset()

# The `@dcl/rpc` runtime closure. Three entry points are named — `@dcl/rpc`
# (`createRpcServer`), `@dcl/rpc/dist/codegen` and
# `@dcl/rpc/dist/transports/WebSocket` by `data-layer-host.mjs`, plus
# `@dcl/rpc/dist/push-channel` by the inspector stand-in's stream — and a
# `require()` walk from those closes over exactly the 12 `.js` below.
#
# The package's whole `require()` surface, across every file it ships, is
# `mitt` and `protobufjs/minimal`. `ts-proto` is in its `dependencies` and is
# never required at runtime — it is the codegen plugin, not a runtime.
#
# 66 files / 314,376 B installed -> 14 files / 110,783 B kept. What goes:
# every `.d.ts` and `.js.map` (the `dist` is mostly declarations and maps),
# `dist/rpc.api.json` (46,818 B of api-extractor report), `dist/protocol/
# index.proto`, `dist/tsdoc-metadata.json`, README. Three runtime files are
# unreachable and go too: `dist/transports/{Memory,WebWorker}.js` (the browser
# and in-process transports — we only ever attach a WebSocket) and
# `dist/codegen-types.js`, which is a 118 B `__esModule` marker for a
# declarations-only module that nothing requires.
RPC_RUNTIME = frozenset({
    'package.json', 'LICENSE',
    'dist/index.js', 'dist/types.js', 'dist/server.js', 'dist/client.js',
    'dist/client-request-dispatcher.js', 'dist/message-dispatcher.js',
    'dist/stream-protocol.js', 'dist/push-channel.js', 'dist/codegen.js',
    'dist/protocol/index.js', 'dist/protocol/helpers.js',
    'dist/transports/WebSocket.js',
})

# Packages shipped as many interchangeable builds, or with a large dead
# surface. Only the files listed can ever be loaded; the resolver check at the
# end of this script proves the kept set is enough.
FILE_ALLOWLIST = {
    'protobufjs': lambda rel: rel in PROTOBUFJS_SHIP_NOTHING,
    'typescript': lambda rel: rel in ('package.json', 'LICENSE.txt')
    or rel in ('lib/tsc.js', 'lib/_tsc.js')
    or (rel.startswith('lib/lib.') and rel.endswith('.d.ts')),
    # Manifest + the shared tsconfigs every scene extends, and nothing else:
    # the implementation is in `prebuilt/*.js` and the declarations are in the
    # rollup. `types/` is a directory, not a single file — 7.26.0 ships
    # `tsconfig.ecs7.json` and `tsconfig.ecs7.strict.json`, and a scene may
    # extend either (`77,-5-tweens-moving-platforms` extends the strict one).
    '@dcl/sdk': lambda rel: rel in ('package.json', 'LICENSE')
    or (rel.startswith('types/') and rel.endswith('.json')),
    # Manifest only. `index.d.ts` is injected later — it is the rollup.
    '@dcl/js-runtime': lambda rel: rel in ('package.json', 'LICENSE'),
    # The node-side CRDT codec only. `dist/` is the browser/ESM build that the
    # prebuilt chunks already contain, and every `.d.ts` in the package is in
    # the rollup.
    '@dcl/ecs': lambda rel: rel in ('package.json', 'LICENSE')
    or (rel.startswith('dist-cjs/')
        and not rel.endswith(DECLARATION_SUFFIXES + DROP_SUFFIXES)),
    # Runtime only; `@dcl/ecs/dist-cjs` requires it, nothing type-references it.
    'long': lambda rel: not rel.endswith(DECLARATION_SUFFIXES + DROP_SUFFIXES),
    '@dcl/rpc': lambda rel: rel in RPC_RUNTIME,
    # Runtime only, same trick as `long`: dropping `index.d.ts` is what leaves
    # the rollup's ambient `declare module "mitt"` in charge of the types.
    'mitt': lambda rel: rel in ('package.json', 'LICENSE', 'dist/mitt.js'),
}

# Reached by the closure walk but never shipped: their runtime is inside
# `prebuilt/{core,smart}.js` and their declarations are inside the types
# rollup, so a copy on disk would be dead weight that tsc might additionally
# prefer over the ambient declarations.
#
# `@dcl/asset-packs` is the interesting one. It is installed (the chunk build
# and the rollup both read it) and then dropped, which is how "asset-packs in
# the base blob" costs 150 KB of chunk + 103 KB of declarations instead of the
# 1.3 MB the package weighs after even the aggressive `bin/` prune.
#
# Do NOT "just ship the .d.ts closure" of `@dcl/asset-packs` instead. Measured:
# 90,874 B of `.d.ts` against 1,286,817 B of `.js` in the package, so it looks
# like a cheap 0.09 MB — but it is 0.09 MB of duplicate, and worse than
# duplicate. Its manifest's `typings` points at `dist/definitions.d.ts`, so an
# on-disk copy resolves and tsc prefers it over the ambient `declare module`
# blocks the rollup emits; and a wholesale copy reintroduces the declarations
# `build_types_rollup()`'s reachability walk deliberately excludes (the
# `admin-toolkit-ui/**` half — nothing public re-exports it), which is the same
# trap `@dcl/react-ecs/dist/reconciler/types.d.ts` sprang with its DOM
# `Document` reference. The TS7016 risk that argument is usually made from is
# already handled where it belongs: drop a `.d.ts` the closure needs and
# `build_types_rollup()` raises on unresolved relative specifiers before this
# script writes anything.
#
# Both paths that name the package resolve against the blob alone, checked by
# `init --node-modules-only` + `build --production` on copies of
# decentraland/sdk7-test-scenes: `0,0-cube-spawner` (a direct
# `import { initAssetPacks } from '@dcl/asset-packs/dist/scene-entrypoint'` in
# the scene's own source, no composite) and `3,2-proximity-interactions` (the
# import `entrypoint.rs` injects for `asset-packs::` composite components).
# Both reach `[5/5] Type check passed`, and the second also reaches
# `[4/5] main.crdt regenerated (1 composite)`.
#
# `mitt` used to be here for the same reason (its runtime is inside the
# chunks). It is not any more: `@dcl/rpc` requires it in NODE, outside every
# chunk. Only `dist/mitt.js` ships — 349 B — and the `.d.ts` still does not, so
# the rollup's ambient declaration keeps owning the types.
DROP_PACKAGES = {
    '@dcl/react-ecs', '@dcl/ecs-math', '@dcl/asset-packs',
    'react', 'react-reconciler', 'scheduler',
    'loose-envify', 'js-tokens',
}

NODE_BUILTINS = {
    'assert', 'async_hooks', 'buffer', 'child_process', 'cluster', 'console',
    'constants', 'crypto', 'dgram', 'diagnostics_channel', 'dns', 'domain',
    'events', 'fs', 'http', 'http2', 'https', 'inspector', 'module', 'net',
    'os', 'path', 'perf_hooks', 'process', 'punycode', 'querystring',
    'readline', 'repl', 'stream', 'string_decoder', 'timers', 'tls',
    'trace_events', 'tty', 'url', 'util', 'v8', 'vm', 'wasi', 'worker_threads',
    'zlib',
}

# Resolved by `linker.rs` at bundle time, not by node_modules.
VIRTUAL_PREFIXES = ('~sdk/', '~system/')

# `ws` requires these two only behind a try/catch; they are optional native
# speedups the pure-JS fallback replaces.
OPTIONAL_SPECS = {'bufferutil', 'utf-8-validate'}

DROP_SUFFIXES = ('.map', '.md', '.markdown', '.flow')
DECLARATION_SUFFIXES = ('.d.ts', '.d.cts', '.d.mts')
DROP_DIRS = {
    'test', 'tests', '__tests__', 'docs', 'example', 'examples',
    'bench', 'benchmark', 'benchmarks', '.github',
}
DROP_FILES = {'eslint.config.js', '.eslintrc.js', '.eslintrc.cjs', 'karma.conf.js'}
# Test files that are not under a test/ directory — `protobufjs/ext/descriptor/
# test.js` is one, and its `require('deep-diff')` would otherwise widen the
# closure by a package nothing ships.
DROP_FILE_RE = re.compile(r'(^|\.)(test|tests|spec)\.(js|cjs|mjs|ts)$')

SPEC_RE = re.compile(
    r'''(?:require\(\s*|(?:^|[\s;{}])(?:import|export)[^'"()]{0,200}?from\s*|import\(\s*)'''
    r'''['"]([^'"\n]+)['"]''',
    re.M,
)
# `/// <reference types="node" />` pulls a whole @types package into the check.
REF_TYPES_RE = re.compile(r'///\s*<reference\s+types\s*=\s*"([^"]+)"')

# Comments must not count as imports. This is not pedantry: the *only* reason
# `ethers` (10.7 MB) was ever vendored is that a scan like this one matched
# `* import { ethers } from 'ethers'` inside the JSDoc header of
# `@dcl/sdk/ethereum-provider/index.js`. Block comments and whole-line `//`
# comments are stripped; a trailing `//` is left alone so a URL inside a
# string can never swallow a real specifier later on the same line.
BLOCK_COMMENT_RE = re.compile(r'/\*[\s\S]*?\*/')
LINE_COMMENT_RE = re.compile(r'^[ \t]*//.*$', re.M)


def strip_comments(text: str) -> str:
    return LINE_COMMENT_RE.sub('', BLOCK_COMMENT_RE.sub('', text))


def scan(text: str) -> set[str]:
    """Bare/deep specifiers and `/// <reference types>` names in one file.

    A reference-types name is tagged `types:x` rather than turned into a
    specifier, because tsc satisfies it from *either* `@types/x` or a package
    literally named `x` — `@dcl/js-runtime` is the latter.
    """
    found = {m.group(1) for m in SPEC_RE.finditer(strip_comments(text))}
    found |= {f'types:{m.group(1)}' for m in REF_TYPES_RE.finditer(text)}
    return found


def types_candidates(spec: str) -> list[str]:
    name = spec[len('types:'):]
    return [name, f'@types/{name}']

SCAN_SKIP_DIRS = {'node_modules'}
SCAN_SUFFIXES = ('.js', '.cjs', '.mjs', '.ts', '.tsx', '.d.ts', '.d.cts', '.d.mts')


def log(msg: str) -> None:
    print(msg, flush=True)


def pkg_of_spec(spec: str) -> str | None:
    if not spec or spec[0] in './' or spec.startswith('node:'):
        return None
    parts = spec.split('/')
    return f'{parts[0]}/{parts[1]}' if parts[0].startswith('@') else parts[0]


def list_packages(nm: str) -> list[str]:
    out = []
    for name in sorted(os.listdir(nm)):
        if name.startswith('.'):
            continue
        full = os.path.join(nm, name)
        if name.startswith('@') and os.path.isdir(full):
            out += [f'{name}/{s}' for s in sorted(os.listdir(full))]
        elif os.path.isdir(full):
            out.append(name)
    return out


def wanted(pkg: str, rel: str) -> bool:
    allow = FILE_ALLOWLIST.get(pkg)
    if allow is not None:
        return allow(rel)
    if rel.endswith(DROP_SUFFIXES):
        return False
    parts = rel.split('/')
    if parts[-1] in DROP_FILES or DROP_FILE_RE.search(parts[-1]):
        return False
    return not any(p in DROP_DIRS for p in parts[:-1])


def specifiers_in(nm: str, pkg: str) -> set[str]:
    """Bare/deep imports of the files this package will actually ship.

    Declarations count: `build --production` type-checks the scene, and tsc
    follows `.d.ts` imports into other packages exactly like node follows
    `require`. A package reached only from a `.d.ts` still has to be there.
    """
    found: set[str] = set()
    root = os.path.join(nm, pkg)
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SCAN_SKIP_DIRS]
        for fn in filenames:
            if not fn.endswith(SCAN_SUFFIXES):
                continue
            path = os.path.join(dirpath, fn)
            rel = os.path.relpath(path, root).replace(os.sep, '/')
            if os.path.islink(path) or not wanted(pkg, rel):
                continue
            with open(path, encoding='utf-8', errors='ignore') as fh:
                found |= scan(fh.read())
    return found


def deps_of_spec(spec: str) -> list[str]:
    if spec.startswith('types:'):
        return types_candidates(spec)
    # A bare specifier that names a node builtin is satisfied by node itself,
    # even when a userland package of the same name happens to be installed —
    # `ws` requires 'buffer', which is *not* the npm `buffer` package.
    pkg = pkg_of_spec(spec)
    return [] if pkg is None or pkg in NODE_BUILTINS else [pkg]


def reachable(nm: str, present: set[str]) -> tuple[set[str], dict[str, str]]:
    keep = {p for p in ENTRY_PACKAGES if p in present}
    why = {p: ENTRY_REASON.get(p, 'entry point') for p in keep}
    queue = list(keep)
    while queue:
        pkg = queue.pop()
        for spec in specifiers_in(nm, pkg):
            for dep in deps_of_spec(spec):
                if (
                    dep in present
                    and dep not in keep
                    and dep not in NEVER
                    and dep not in DROP_PACKAGES
                ):
                    keep.add(dep)
                    why[dep] = f'{pkg} imports {spec!r}'
                    queue.append(dep)
    return keep, why


def collect(nm: str, keep: set[str]) -> tuple[dict[str, bytes], dict[str, int]]:
    files: dict[str, bytes] = {}
    kept_bytes: dict[str, int] = {}
    for pkg in sorted(keep):
        root = os.path.join(nm, pkg)
        if not os.path.isdir(root):
            continue
        total = 0
        for dirpath, dirnames, filenames in os.walk(root):
            # A nested node_modules is a version conflict with the hoisted
            # tree. Everything reached here resolves at top level (the
            # resolver check proves it), so the nested copies are dead.
            dirnames[:] = [d for d in dirnames if d != 'node_modules']
            for fn in filenames:
                src = os.path.join(dirpath, fn)
                if os.path.islink(src):
                    continue
                rel = os.path.relpath(src, root).replace(os.sep, '/')
                if not wanted(pkg, rel):
                    continue
                with open(src, 'rb') as fh:
                    data = fh.read()
                files[f'node_modules/{pkg}/{rel}'] = data
                total += len(data)
        kept_bytes[pkg] = total
    return files, kept_bytes


# --------------------------------------------------------------------------
# the ambient declaration rollup
# --------------------------------------------------------------------------

# package -> (declaration subdir, types entry relative to the package root).
# `@dcl/js-runtime` is absent on purpose: its three files are already ambient
# and are copied verbatim at top level, see `build_types_rollup()`.
ROLLUP_PKGS = [
    ('@dcl/sdk', '', 'index'),
    ('@dcl/ecs', 'dist', 'dist/index'),
    ('@dcl/react-ecs', 'dist', 'dist/index'),
    ('@dcl/ecs-math', 'dist', 'dist/index'),
    ('@dcl/asset-packs', 'dist', 'dist/definitions'),
    ('mitt', '', 'index'),
    ('protobufjs', '', 'index'),
    ('long', '', 'index'),
    ('@protobufjs/aspromise', '', 'index'),
    ('@protobufjs/base64', '', 'index'),
    ('@protobufjs/eventemitter', '', 'index'),
    ('@protobufjs/float', '', 'index'),
    ('@protobufjs/inquire', '', 'index'),
    ('@protobufjs/pool', '', 'index'),
    ('@protobufjs/utf8', '', 'index'),
]

# Packages with no `exports` map, where therefore EVERY declaration file is a
# public subpath a scene may name. `@dcl/sdk/network/binary-message-bus` is one
# such: `network/index.d.ts` does not re-export it, so BFS from the entry points
# alone would miss it.
ROLLUP_FULL_SUBPATH_PKGS = ('@dcl/sdk', '@dcl/asset-packs')

ROLLUP_SPEC_RE = re.compile(
    r"""(?P<pre>\bfrom\s*|\bimport\s*\(\s*|\bimport\s+|\brequire\s*\(\s*)"""
    r"""(?P<q>['"])(?P<spec>[^'"]+)(?P=q)"""
)
ROLLUP_DECLARE_RE = re.compile(
    r'\bdeclare\s+(?=(?:abstract\s+)?'
    r'(?:const|let|var|function|class|namespace|enum|interface|type|module|global)\b)'
)
ROLLUP_EXPORT_AS_NS_RE = re.compile(r'^\s*export\s+as\s+namespace\s+\w+\s*;?\s*$', re.M)
ROLLUP_REF_RE = re.compile(r'^///\s*<reference[^>]*/>\s*$', re.M)


def build_types_rollup(nm: str) -> bytes:
    """Roll the scene-facing `.d.ts` of the SDK packages into ONE ambient file.

    Every kept file becomes `declare module "<canonical specifier>" { ... }`
    with its relative imports rewritten to canonical specifiers, so tsc resolves
    `@dcl/sdk/ecs` & co. from ambient module declarations instead of from a
    `node_modules` tree — which is what lets the packages themselves go.

    `@dcl/js-runtime`'s own three declarations are ambient already (global
    interfaces + `declare module '~system/*'` + `declare module '~sdk/*'`) and
    are copied verbatim at top level. That is load-bearing: the bundle then has
    no top-level `import`/`export` and stays a *script*, which is what makes the
    inner `declare module` blocks ambient declarations rather than
    augmentations of modules that no longer exist.

    The keep-set is tsc reachability, not "every `.d.ts` in the package": BFS
    from the public entry points, following declaration imports the way tsc
    does. Shipping the unreachable ones would *add* errors the node_modules tree
    never produced — `@dcl/react-ecs/dist/reconciler/types.d.ts` names the DOM
    type `Document`, which `lib: ["ES2020"]` does not define, and tsc never
    loads that file today because nothing public re-exports it.

    Six transforms are forced by the re-homing. Each is an outright tsc error
    otherwise:

    * `declare` modifiers are stripped inside the module bodies (559
      occurrences). A `.d.ts` at top level is not an ambient context, so
      upstream writes `export declare const`; inside `declare module { }` it
      already is one and TS1038 rejects the modifier.
    * `export as namespace X` (the UMD global of protobufjs and long) is
      dropped: TS1316, "global module exports may only appear at top level".
    * An alias module whose target uses `export =` (protobufjs, long,
      `@protobufjs/*`) is emitted as `import x = require(...); export = x`.
      `export *` against such a module is TS2498.
    * `@dcl/js-runtime/apis.d.ts` reaches @dcl/ecs as `import('../ecs')` — it
      walks out of its own package directory, which only resolves while the two
      are siblings under `node_modules/@dcl/`, exactly what this bundle
      removes. Rewritten to `import('@dcl/ecs')`.
    * A relative specifier written with a `.js` extension (long's `./types.js`)
      resolves to the sibling `.d.ts`.
    * `@dcl/sdk/src/<x>` is aliased onto `@dcl/sdk/<x>`. `@dcl/sdk` ships its
      own TypeScript *sources* under `src/` and real scenes import them —
      `@dcl/sdk/src/players` and `@dcl/sdk/src/network`, two scenes in
      decentraland/sdk7-test-scenes. Those `.ts` carry implementations and
      cannot become `declare module` bodies. With the 33 alias blocks (~3 KB)
      the corpus is 60/60; without them, 58/60.
    """
    known: dict[str, str] = {}   # abs path -> canonical specifier

    for pkg, subdir, _entry in ROLLUP_PKGS:
        root = os.path.join(nm, pkg)
        if not os.path.isdir(root):
            continue
        base = os.path.join(root, subdir) if subdir else root
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames
                           if d not in ('dist-cjs', 'src', 'etc', 'node_modules')]
            for fn in filenames:
                if not fn.endswith('.d.ts'):
                    continue
                p = os.path.join(dirpath, fn)
                rel = os.path.relpath(p, root)[: -len('.d.ts')].replace(os.sep, '/')
                known[p] = f'{pkg}/{rel}'

    def specifiers(text: str) -> list[str]:
        return [m.group('spec') for m in ROLLUP_SPEC_RE.finditer(text)]

    def resolve_rel(from_file: str, spec: str) -> str | None:
        base = os.path.normpath(os.path.join(os.path.dirname(from_file), spec))
        cands = [base + '.d.ts', os.path.join(base, 'index.d.ts')]
        if base.endswith('.js'):
            cands.insert(0, base[:-3] + '.d.ts')
        return next((c for c in cands if c in known), None)

    def resolve_bare(spec: str) -> str | None:
        for p, canon in known.items():
            if canon == spec or canon == spec + '/index':
                return p
        for pkg, _subdir, entry in ROLLUP_PKGS:
            if spec == pkg:
                p = os.path.join(nm, pkg, entry + '.d.ts')
                if p in known:
                    return p
        return None

    entry_points = []
    for pkg, _subdir, entry in ROLLUP_PKGS:
        p = os.path.join(nm, pkg, entry + '.d.ts')
        if p in known:
            entry_points.append(p)
    for pkg in ROLLUP_FULL_SUBPATH_PKGS:
        for dirpath, dirnames, filenames in os.walk(os.path.join(nm, pkg)):
            dirnames[:] = [d for d in dirnames
                           if d not in ('src', 'types', 'bin', 'node_modules')]
            entry_points += [os.path.join(dirpath, f)
                             for f in filenames if f.endswith('.d.ts')]

    seen: set[str] = set()
    texts: dict[str, str] = {}
    queue = list(entry_points)
    while queue:
        p = queue.pop()
        if p in seen or p not in known:
            continue
        seen.add(p)
        with open(p, encoding='utf8') as fh:
            texts[p] = fh.read()
        for spec in specifiers(texts[p]):
            t = resolve_rel(p, spec) if spec.startswith('.') else resolve_bare(spec)
            if t and t not in seen:
                queue.append(t)
    keep = seen

    aliases: list[tuple[str, str]] = []
    for pkg, _subdir, entry in ROLLUP_PKGS:
        p = os.path.join(nm, pkg, entry + '.d.ts')
        if p in keep:
            aliases.append((pkg, known[p]))
    for p in keep:
        canon = known[p]
        if canon.endswith('/index'):
            aliases.append((canon[: -len('/index')], canon))
    sdk_src = os.path.join(nm, '@dcl/sdk/src')
    canon_of_keep = {known[p] for p in keep}
    for dirpath, _dn, filenames in os.walk(sdk_src):
        for f in filenames:
            if not f.endswith('.ts') or f.endswith('.d.ts'):
                continue
            rel = os.path.relpath(os.path.join(dirpath, f), sdk_src)[:-3]
            rel = rel.replace(os.sep, '/')
            targets = [f'@dcl/sdk/{rel}']
            if rel.endswith('/index'):
                targets.append(f'@dcl/sdk/{rel[: -len("/index")]}')
            for target in targets:
                if target in canon_of_keep:
                    aliases.append((f'@dcl/sdk/src/{rel}', target))
                    if rel.endswith('/index'):
                        aliases.append(
                            (f'@dcl/sdk/src/{rel[: -len("/index")]}', target))
                    break

    out = [
        '// GENERATED by scripts/build-base-blob.py -- do not edit.',
        '// Ambient declaration bundle: @dcl/sdk, @dcl/ecs, @dcl/react-ecs,',
        '// @dcl/ecs-math, @dcl/asset-packs, @dcl/js-runtime + the mitt,',
        '// protobufjs and long declarations their .d.ts reach. Replaces those',
        '// packages\' .d.ts trees entirely; the implementations live in',
        f'// {CORE_CHUNK} and {SMART_CHUNK}.',
        '',
    ]
    for name in ('index.d.ts', 'apis.d.ts', 'sdk.d.ts'):
        p = os.path.join(nm, '@dcl/js-runtime', name)
        if not os.path.exists(p):
            continue
        with open(p, encoding='utf8') as fh:
            txt = ROLLUP_REF_RE.sub('', fh.read())
        txt = txt.replace("'../ecs'", "'@dcl/ecs'").replace('"../ecs"', '"@dcl/ecs"')
        out += [f'// ---- @dcl/js-runtime/{name} (ambient, verbatim) ----', txt, '']

    unresolved: dict[str, list[str]] = {}
    uses_export_eq: set[str] = set()
    for p in sorted(keep):
        body = texts[p]
        bad = [s for s in specifiers(body)
               if s.startswith('.') and not resolve_rel(p, s)]
        if bad:
            unresolved[os.path.relpath(p, nm)] = bad

        def sub(m: re.Match, _p: str = p) -> str:
            spec = m.group('spec')
            if not spec.startswith('.'):
                return m.group(0)
            t = resolve_rel(_p, spec)
            if t is None:
                return m.group(0)
            return f'{m.group("pre")}{m.group("q")}{known[t]}{m.group("q")}'

        body = ROLLUP_SPEC_RE.sub(sub, body)
        body = ROLLUP_REF_RE.sub('', body)
        body = ROLLUP_EXPORT_AS_NS_RE.sub('', body)
        body = ROLLUP_DECLARE_RE.sub('', body)
        if re.search(r'^\s*export\s*=', body, re.M):
            uses_export_eq.add(known[p])
        indented = '\n'.join(('  ' + l) if l.strip() else l
                             for l in body.splitlines())
        out += [f'declare module "{known[p]}" {{', indented, '}', '']

    emitted: set[str] = set()
    for alias, canon in sorted(set(aliases)):
        if alias in emitted or alias == canon or alias in canon_of_keep:
            continue
        emitted.add(alias)
        if canon in uses_export_eq:
            out += [f'declare module "{alias}" {{',
                    f'  import __x = require("{canon}");',
                    '  export = __x;', '}', '']
            continue
        src = texts[next(p for p in keep if known[p] == canon)]
        has_default = bool(re.search(
            r'^\s*export\s+default\b|^\s*export\s*\{[^}]*\bdefault\b', src, re.M))
        body_lines = [f'  export * from "{canon}";']
        if has_default:
            body_lines.append(f'  export {{ default }} from "{canon}";')
        out += [f'declare module "{alias}" {{'] + body_lines + ['}', '']

    if unresolved:
        raise SystemExit(
            'the declaration rollup has unresolved relative specifiers:\n  '
            + '\n  '.join(f'{p}: {u}' for p, u in sorted(unresolved.items())))
    log(f'declaration rollup: {len(keep)} modules of {len(known)} found, '
        f'{len(emitted)} aliases')
    return '\n'.join(out).encode('utf8')


CHUNK_SCENE_JSON = (
    '{"runtimeVersion":"7","main":"bin/index.js",'
    '"scene":{"parcels":["0,0"],"base":"0,0"}}'
)
CHUNK_TSCONFIG = (
    '{"compilerOptions":{"jsx":"react","allowJs":true,"resolveJsonModule":true,'
    '"strict":true},"include":["src/**/*.ts"],'
    '"extends":"@dcl/sdk/types/tsconfig.ecs7.json"}'
)


def build_chunks(work: str, sdk_bin: str, files: dict[str, bytes],
                 kept_bytes: dict[str, int]) -> None:
    """Bundle the two prebuilt runtime chunks and add them to the blob.

    The chunks are produced by `dcl-one-sdk vendor-chunks`, not here: the
    rolldown settings (`CodeSplittingMode::Bool(false)`, Cjs, es2020, minify, no
    sourcemap), the registry key lists and the `~sdk/script-utils` alias all
    live in `src/{split,prebuilt}.rs`, and a second copy of them in python would
    drift. This function only stages a throwaway scene against the *unpruned*
    install tree and shells out.

    That tree has to be unpruned for two reasons the pruned one cannot satisfy:
    the core chunk bundles all of `@dcl/{sdk,ecs,react-ecs,ecs-math}` + react,
    and the smart chunk bundles `@dcl/asset-packs` *and*
    `@dcl/sdk-commands/dist/logic/runtime-script.js` — the real
    `~sdk/script-utils` runtime, from a package `NEVER` keeps.

    `vendor-chunks` verifies both chunks before returning: every `require()`
    either chunk emits must be `~system/*` or a key the loader's registry will
    hold when that chunk is evaluated. That check is what caught
    `@dcl/sdk/platform` and `@dcl/sdk/text-codec` — required by asset-packs,
    invisible while it shared a chunk with the SDK, and a hard
    "not in the sdk runtime registry" throw at scene start once it did not.
    """
    if not os.path.isfile(sdk_bin):
        raise SystemExit(
            f'{sdk_bin} does not exist.\n'
            'The chunk build needs a dcl-one-sdk binary. Build one first:\n'
            '  cargo build -p dcl-one-sdk --release\n'
            'There is no bootstrap cycle: that binary only has to contain '
            'rolldown and src/split.rs, not the blob it will later ship.'
        )
    scene = os.path.join(work, 'chunk-scene')
    shutil.rmtree(scene, ignore_errors=True)
    os.makedirs(os.path.join(scene, 'src'))
    os.symlink(os.path.join(work, 'node_modules'),
               os.path.join(scene, 'node_modules'))
    with open(os.path.join(scene, 'scene.json'), 'w') as fh:
        fh.write(CHUNK_SCENE_JSON)
    with open(os.path.join(scene, 'tsconfig.json'), 'w') as fh:
        fh.write(CHUNK_TSCONFIG)
    with open(os.path.join(scene, 'src/index.ts'), 'w') as fh:
        fh.write('export function main() {}\n')

    out = os.path.join(work, 'prebuilt')
    shutil.rmtree(out, ignore_errors=True)
    log('building the prebuilt runtime chunks')
    r = subprocess.run(
        [sdk_bin, 'vendor-chunks', '--dir', scene,
         '--out-core', os.path.join(out, 'core.js'),
         '--out-smart', os.path.join(out, 'smart.js')],
        capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f'vendor-chunks failed:\n{r.stdout}\n{r.stderr}')

    total = 0
    for name, rel in (('core.js', CORE_CHUNK), ('smart.js', SMART_CHUNK),
                      ('registry.json', CHUNK_REGISTRY)):
        with open(os.path.join(out, name), 'rb') as fh:
            data = fh.read()
        files[rel] = data
        total += len(data)
        log(f'    {rel}  {len(data)} B')
    kept_bytes['@dcl/sdk (prebuilt chunks)'] = total


PBMIN_ENTRY = b'module.exports = require("./index.js");\n'
PBMIN_MANIFEST = {
    'name': 'protobufjs',
    # NOT 7.2.4. Nothing in the blob reads this field, but a human running
    # `npm ls` in an extracted scene must not be told they have upstream 7.2.4.
    'version': '7.2.4-dcl-one-sdk-pbmin.1',
    'description': 'protobufjs/minimal wire codec, reimplemented dependency-free '
                   'for dcl-one-sdk. NOT upstream protobufjs. See '
                   'experiments/protobufjs-minimal-replacement.',
    'main': 'index.js',
    'license': 'BSD-3-Clause',
}


PBMIN_TREE_ENTRY = b'''// Redirected by scripts/build-base-blob.py swap_pbmin_into_tree().
// NOT upstream's minimal entry point. See ../protobufjs/pbmin.js.
"use strict";
module.exports = require("./pbmin.js");
'''


def swap_pbmin_into_tree(work: str) -> None:
    """Point the INSTALL TREE's `protobufjs/minimal` at the replacement too.

    `add_pbmin()` decides what the blob *ships*; this decides what the prebuilt
    chunks are *built against*. They used to disagree: rolldown resolved
    `protobufjs/minimal` through this tree and bundled UPSTREAM into
    `prebuilt/core.js`, so an extracted scene carried two protobuf codecs — ours
    in `node_modules`, upstream's inside the chunk — and the scene runtime, the
    one place a codec bug is visible in every scene rather than in one
    dev-machine process, ran the one with no owner.

    Two codecs is the thing worth removing, not the 25 KB. This makes it one.

    Mechanics: protobufjs 7.2.4 has NO `exports` map (checked, not assumed), so
    `protobufjs/minimal` resolves plainly to `minimal.js` and overwriting that
    file is the whole intervention. The replacement lands beside it as
    `pbmin.js` rather than clobbering `index.js`, which stays upstream's full
    reflection build:

      * `build_types_rollup()` still reads this package's `.d.ts` files, and
        `index.d.ts` is what `main` points at;
      * a `require('protobufjs')` for the reflection library still gets the real
        thing, so if something ever does reach for it the failure is a missing
        *dependency*, not a silently different library.

    Only `minimal.js` moves, and only in the throwaway `--work` tree, which is
    rebuilt from a pnpm install on every run. Nothing here touches a checkout.

    Verified by `check_chunk_pbmin()` below, which fails the build if the
    provenance marker is absent from `prebuilt/core.js`.
    """
    pkg = os.path.join(work, 'node_modules', 'protobufjs')
    entry = os.path.join(pkg, 'minimal.js')
    if not os.path.isfile(entry):
        raise SystemExit(
            f'{entry} does not exist — the install tree has no protobufjs to '
            'redirect. The package must stay in EXTRA_INSTALL/the scaffold pins '
            'even though nothing of it ships; see the PBMIN comment above NEVER.')
    with open(os.path.join(PBMIN, 'index.js'), 'rb') as fh:
        core = fh.read()
    with open(os.path.join(pkg, 'pbmin.js'), 'wb') as fh:
        fh.write(core)
    with open(entry, 'wb') as fh:
        fh.write(PBMIN_TREE_ENTRY)
    log(f'    install tree: protobufjs/minimal.js -> pbmin.js  {len(core)} B '
        '(prebuilt chunks now bundle the replacement)')


# The marker `experiments/protobufjs-minimal-replacement/index.js` sets on the
# exported namespace. A string literal survives rolldown's minifier where every
# other candidate does not: the error messages are byte-identical to upstream on
# purpose, the property names are all reproduced, and the require paths that DO
# differ are erased by bundling. Without this the only way to tell which codec a
# chunk contains is to diff it against a reference build.
PBMIN_MARKER = b'dcl-one-sdk-pbmin.1'


def check_chunk_pbmin(files: dict[str, bytes]) -> None:
    """Fail the build if `prebuilt/core.js` does not contain the replacement.

    This exists because the failure it catches is silent. `swap_pbmin_into_tree()`
    works by file overwrite, so anything that reorders the pipeline, adds an
    `exports` map upstream, or re-installs between the swap and the chunk build
    puts upstream's codec back into the scene runtime with no error and no size
    change worth noticing. The two-codec state this change removed is exactly the
    state that would come back.
    """
    chunk = files.get(CORE_CHUNK)
    if chunk is None:
        raise SystemExit(f'{CORE_CHUNK} missing from the blob')
    if PBMIN_MARKER not in chunk:
        raise SystemExit(
            f'{CORE_CHUNK} does not contain {PBMIN_MARKER.decode()} — the scene '
            'runtime is bundling a protobuf codec that is not the replacement.\n'
            'swap_pbmin_into_tree() must run against the same tree build_chunks() '
            'resolves against, and before it.')

    # Rolldown renames top-level bindings unless something forbids it, and the
    # only thing that does is a DIRECT eval: it concatenates every module into
    # one scope, so one eval anywhere preserves every name in the bundle. These
    # helper names are rolldown's own, emitted mangled (`e`, `t`, `n`) in a
    # healthy build and verbatim in a poisoned one — so they are a cheap,
    # specific probe for the condition.
    #
    # This is not hypothetical tidiness. pbmin originally reproduced upstream's
    # `eval("quire".replace(/^/, "re"))` trick verbatim, and that one line cost
    # +93,824 B (+20.2%) on the chunk every scene ships — invisible in the size
    # report, which only ever showed the chunk getting bigger by "some" amount.
    # See `inquire()` in experiments/protobufjs-minimal-replacement/index.js.
    unmangled = [n for n in (b'__getOwnPropNames', b'__defProp', b'__hasOwnProp')
                 if n in chunk]
    if unmangled:
        raise SystemExit(
            f'{CORE_CHUNK} still has unmangled top-level names '
            f'({", ".join(n.decode() for n in unmangled)}), so rolldown gave up on '
            'renaming the whole bundle.\nAlmost always a direct eval() reintroduced '
            'into a bundled module — check inquire() in the pbmin source. Expect '
            'roughly +20% on this chunk while it lasts.')
    log(f'  chunk codec check: {CORE_CHUNK} contains the pbmin marker, '
        'top-level names mangled')


def add_pbmin(files: dict[str, bytes], kept_bytes: dict[str, int]) -> None:
    """Ship OUR `protobufjs/minimal` instead of upstream's.

    `node_modules/protobufjs/{package.json,index.js,minimal.js}`, where
    `index.js` is `experiments/protobufjs-minimal-replacement/index.js`
    verbatim and `minimal.js` is a one-line re-export — the same shape as
    upstream's own `minimal.js`, so `require('protobufjs/minimal')` resolves to
    a file with the same name and no `exports` map is introduced.

    **What this replaces.** 13 files / 58,872 B of upstream `protobufjs` plus
    six whole `@protobufjs/*` micro-packages (`aspromise`, `base64`,
    `eventemitter`, `float`, `inquire`, `pool` — 25 files / 41,117 B) that only
    it imported. 38 files / 99,989 B out, 4 files / 48,340 B in — net -34 files,
    -51,649 B unpacked, -25,873 B zipped (447 -> 413 files, 2,429,940 ->
    2,404,067 B).

    **Who reads it.** Three consumers in NODE:

    * `@dcl/ecs/dist-cjs` — 69 files, the CRDT wire format `data_layer.rs`
      regenerates `main.crdt` through;
    * `@dcl/rpc` — 6 files, the data-layer socket's framing;
    * `@dcl/inspector/data-layer.gen.js` — the 22-method service descriptor
      `build_service_descriptor()` emits.

    And, since `swap_pbmin_into_tree()`, the SCENE runtime as well: the same
    code is bundled into `prebuilt/core.js`, so QuickJS runs it too. That was
    once the argument for this being low-risk — it isn't any more, and the
    honest framing is that the risk moved onto the scene protocol's critical
    path in exchange for the toolchain having one codec instead of two.

    **Evidence.** `experiments/protobufjs-minimal-replacement/tests`, run with
    `tests/setup.sh && bash tests/run-all.sh <seed>`. Per seed, against
    upstream 7.2.4 as the reference, asserting byte-identical encode in both
    directions, deep-equal decodes and throw-parity including error message:

    * all 336 `@dcl/ecs` message namespaces (142 unique types) x 300 instances
      x 4 environments (Buffer/no-Buffer x Long/no-Long) = 403,200 instances;
    * the ESM `dist/` build of the same corpus, 244 namespaces x 100;
    * all 41 namespaces of the two catalogues that are NOT `@dcl/ecs`
      (`@dcl/rpc/dist/protocol/index.js`, 11; the data-layer descriptor, 30)
      x 2000 x the same 4 environments = 328,000 instances, plus an assertion
      that all 22 descriptor methods' request and response types were among
      them;
    * 800,000 fuzz operations on random/truncated/corrupted input, where the
      two implementations must agree on WHICH inputs throw.

    Zero divergences, six seeds. Mutation-tested: 13 injected wire-format bugs,
    13 caught, and the rpc/descriptor phase catches two of them
    (`varint-length-boundary-off-by-one`, `fixed32-byte-order`) that the
    `@dcl/ecs` corpus phase does not.

    Two upstream inconsistencies are reproduced ON PURPOSE and must not be
    "fixed": `BufferWriter.string` and `Writer.string` encode lone surrogates
    differently, and `BufferReader.string` clamps a truncated length with
    `Math.min` where `Reader.string` throws.
    """
    # `collect()` walked the installed protobufjs and kept nothing, leaving a 0 B
    # entry in the size report. Drop it so the report names one protobufjs.
    kept_bytes.pop('protobufjs', None)
    total = 0
    src = os.path.join(PBMIN, 'index.js')
    with open(src, 'rb') as fh:
        core = fh.read()
    # The replacement reproduces protobuf.js's API and wire behaviour, so it is a
    # derivative work and upstream's BSD-3-Clause notice has to travel with it.
    with open(os.path.join(PBMIN, 'LICENSE'), 'rb') as fh:
        licence = fh.read()
    out = {
        'node_modules/protobufjs/package.json':
            json.dumps(PBMIN_MANIFEST, indent=2).encode() + b'\n',
        'node_modules/protobufjs/LICENSE': licence,
        'node_modules/protobufjs/index.js': core,
        'node_modules/protobufjs/minimal.js': PBMIN_ENTRY,
    }
    for name, data in out.items():
        files[name] = data
        total += len(data)
    kept_bytes['protobufjs (pbmin)'] = total
    log(f'    node_modules/protobufjs <- {os.path.relpath(src, CRATE)}  '
        f'{len(core)} B, {len(out)} files')


# The service descriptor's SOURCE. Not shipped: `build_service_descriptor()`
# transpiles it and only the `.js` lands in the blob. The `.proto` beside it is
# not shipped either — it is there so the next person can see what the
# descriptor was generated from without a creator-hub checkout.
SHIM_SOURCE_ONLY = ('.ts', '.proto')
# The shim's own type check (`scripts/check-editor-host.sh`) configures itself
# from this. It describes how to check the source, so it belongs beside the
# source and not in a blob a scene unpacks.
SHIM_SOURCE_ONLY_NAMES = ('tsconfig.json',)
SHIM_GEN_TS = 'data-layer.gen.ts'
SHIM_GEN_JS = 'node_modules/@dcl/inspector/data-layer.gen.js'


def add_shim(files: dict[str, bytes], kept_bytes: dict[str, int]) -> None:
    """The `@dcl/inspector` stand-in: crdt dumper + minimal data-layer host.

    `dump-crdt` in `data-layer-host.mjs` needs `dumpEngineToCrdtCommands`, and
    its `serve` mode needs `createDataLayerHost` + `DataServiceDefinition`. The
    real package is 119 MB — ~90% browser editor UI — and ships separately as
    `inspector.zip`. Without this, every `.composite` scene fails build step 4
    with `Cannot find module '@dcl/inspector'`, and `start --data-layer` has no
    host to spawn.

    Two of the files copied here are generated, not written: see
    `scripts/dump-inspector-tables.cjs` for `component-schemas.json` and
    `minimal-composite.json`. Both are version-pinned snapshots of a real
    `@dcl/inspector` and go stale silently when upstream adds a component —
    re-run that script on every inspector bump.
    """
    total = 0
    for dirpath, _, filenames in os.walk(SHIM):
        for fn in sorted(filenames):
            src = os.path.join(dirpath, fn)
            rel = os.path.relpath(src, SHIM).replace(os.sep, '/')
            if rel.endswith(SHIM_SOURCE_ONLY) or rel in SHIM_SOURCE_ONLY_NAMES:
                continue
            with open(src, 'rb') as fh:
                data = fh.read()
            files[f'node_modules/@dcl/inspector/{rel}'] = data
            total += len(data)
    kept_bytes['@dcl/inspector'] = total


def build_service_descriptor(work: str, files: dict[str, bytes],
                             kept_bytes: dict[str, int]) -> None:
    """Transpile the data-layer service descriptor to CommonJS.

    `codegen.registerService` needs a descriptor: 22 methods, each with a
    `name`, `requestStream`/`responseStream` flags and a request/response type
    that can `encode`/`decode`. Upstream generates it from
    `packages/inspector/src/lib/data-layer/proto/data-layer.proto` with
    `protoc-gen-dcl_ts_proto` — a plugin from **`@dcl/ts-proto`, a DCL FORK**,
    not the `ts-proto` on npm — under
    `esModuleInterop=true,returnObservable=false,
    outputServices=generic-definitions,fileSuffix=.gen,oneof=unions,
    useMapType=true`.

    We vendor the OUTPUT (`src/vendor/inspector-shim/data-layer.gen.ts`,
    75,703 B, checked in verbatim beside the `.proto` it came from) and
    transpile it here, rather than reproducing the toolchain. Reproducing it
    would mean vendoring a ~5 MB Node codegen plugin plus `ts-proto-descriptors`
    / `case-anything` / `dprint-node` to regenerate a file that changes about
    once a year — the proto's last edit is 2025-11-13 (creator-hub 36c4130).

    Hand-writing the descriptor instead is technically possible (`codegen.js`
    reads only those five fields per method) but would be 22 hand-maintained
    message codecs matching a fork's exact wire choices, for no size win over
    83 KB.

    The transpile has no dependencies of its own: the generated file's ONLY
    imports are `long` and `protobufjs/minimal`, both already in the blob.
    `--noCheck` is what makes this a *transpile* — the two imports resolve to
    packages whose `.d.ts` this blob deliberately drops, so a type check would
    report TS7016 on them and exit non-zero while emitting the same bytes.
    Checking them is not our job anyway; the file is upstream's generated
    artifact and was type-checked where it was generated.
    """
    src = os.path.join(SHIM, SHIM_GEN_TS)
    out = os.path.join(work, 'descriptor')
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    tsc = os.path.join(work, 'node_modules/typescript/lib/tsc.js')
    r = subprocess.run(
        ['node', tsc, '--noCheck', '--module', 'commonjs', '--target', 'es2020',
         '--esModuleInterop', '--skipLibCheck', '--outDir', out, src],
        capture_output=True, text=True)
    emitted = os.path.join(out, SHIM_GEN_TS[:-3] + '.js')
    if r.returncode != 0 or not os.path.isfile(emitted):
        sys.stderr.write(f'descriptor transpile failed:\n{r.stdout}\n{r.stderr}\n')
        raise SystemExit(1)
    with open(emitted, 'rb') as fh:
        data = fh.read()
    # A descriptor missing a method is not a runtime error but a connection
    # kill: `registerService` does `mod[key].bind(mod)` for every key it finds.
    n = data.count(b'requestStream:')
    if n != 22:
        raise SystemExit(f'{SHIM_GEN_JS}: expected 22 methods, found {n}')
    files[SHIM_GEN_JS] = data
    kept_bytes['@dcl/inspector'] = kept_bytes.get('@dcl/inspector', 0) + len(data)
    log(f'    {SHIM_GEN_JS}  {len(data)} B, {n} methods')


ECS7_TSCONFIG = 'node_modules/@dcl/sdk/types/tsconfig.ecs7.json'


def patch_ecs7_tsconfig(files: dict[str, bytes]) -> None:
    """Make the SDK's shared tsconfig TypeScript 6/7 clean.

    EVERY scene extends this file — ours and third-party alike — so a
    deprecated option here lands in every scene's effective config. Fixing it
    at the source is what makes existing scenes keep type-checking, not just
    newly scaffolded ones. Three edits, none of which change behaviour:

    `downlevelIteration: true` -> removed. Emit-only, and inert at the `target:
    es2020` set three lines above it: tsc's checker gates every read behind
    `languageVersion < ES2015`. Our type check runs `tsc --noEmit` anyway.
    Verified by identical emit for a Map/Set/generator/spread probe and
    byte-identical `bin/scene.js` across all 60 sdk7-test-scenes.

    `moduleResolution: "node"` -> `"bundler"`. "node" is what tsc reports as
    the deprecated node10 mode. `bundler` is the honest description of this
    toolchain (rolldown does the resolving) and is legal here because `module`
    is already `esnext`. It still resolves @dcl/sdk's subpath exports,
    `~system/*` virtuals and @dcl/js-runtime ambient types.

    `suppressExcessPropertyErrors: false` -> removed. It is already the
    compiler default, so dropping it is a no-op; TS 7 rejects the option name
    outright (TS5023).

    None of these can be silenced with `"ignoreDeprecations"`: TS 7 removes or
    rejects all three, and TS 5.9.3 rejects that option itself (TS5103).

    The real fix belongs upstream in decentraland/js-sdk-toolchain; until it
    lands this overlay keeps scenes building. See `docs/ts7-migration.md`.
    """
    raw = files[ECS7_TSCONFIG].decode('utf-8')
    before = json.loads(raw)['compilerOptions']
    out = raw

    if '"downlevelIteration"' in out:
        out = re.sub(r'[ \t]*"downlevelIteration"\s*:\s*true,\n', '', out, count=1)
    if '"suppressExcessPropertyErrors"' in out:
        out = re.sub(r'[ \t]*"suppressExcessPropertyErrors"\s*:\s*false,\n', '', out, count=1)
    out = out.replace('"moduleResolution": "node"', '"moduleResolution": "bundler"', 1)

    after = json.loads(out)['compilerOptions']  # must stay valid JSON
    for gone in ('downlevelIteration', 'suppressExcessPropertyErrors'):
        if gone in after:
            raise SystemExit(f'{ECS7_TSCONFIG}: failed to drop {gone}')
    if after.get('moduleResolution') != 'bundler':
        raise SystemExit(f'{ECS7_TSCONFIG}: failed to set moduleResolution=bundler')
    if after.get('module') != 'esnext':
        raise SystemExit(f'{ECS7_TSCONFIG}: moduleResolution=bundler needs module=esnext')
    # nothing else may shift
    untouched = {'downlevelIteration', 'suppressExcessPropertyErrors', 'moduleResolution'}
    if {k: v for k, v in before.items() if k not in untouched} != {
        k: v for k, v in after.items() if k not in untouched
    }:
        raise SystemExit(f'{ECS7_TSCONFIG}: patch changed an unrelated option')

    files[ECS7_TSCONFIG] = out.encode('utf-8')


def resolvable(spec: str, files: set[str]) -> bool:
    if spec.startswith(VIRTUAL_PREFIXES):
        return True
    if spec.startswith('types:'):
        return any(
            any(f.startswith(f'node_modules/{c}/') for f in files)
            for c in types_candidates(spec)
        )
    pkg = pkg_of_spec(spec)
    if pkg is None:
        return True  # relative / builtin — resolved within its own package
    if pkg in NODE_BUILTINS or spec in OPTIONAL_SPECS:
        return True
    prefix = f'node_modules/{spec}'
    if spec == pkg:
        return any(f.startswith(f'node_modules/{pkg}/') for f in files)
    return any(
        f == prefix
        or f in (f'{prefix}.js', f'{prefix}.cjs', f'{prefix}.mjs',
                 f'{prefix}.json', f'{prefix}.d.ts')
        or f.startswith(f'{prefix}/')
        for f in files
    )


SCAN_EXEMPT = {
    # A whole bundled compiler; its specifiers are all builtins behind dynamic
    # requires.
    'node_modules/typescript/lib/_tsc.js',
    # The prebuilt chunks. Their imports are NOT resolved against node_modules —
    # the split loader resolves them against the registry the previous chunk
    # published — so `resolvable()` is the wrong question to ask of them.
    # `check_chunk_registry()` asks the right one instead, and
    # `vendor-chunks` has already asked it a third time from inside rolldown.
    CORE_CHUNK,
    SMART_CHUNK,
    # One ambient declaration bundle whose module names are the specifiers.
    # They are the packages this blob deliberately does not ship; resolving them
    # against node_modules would fail by design.
    TYPES_ROLLUP,
}


def specifiers_in_files(files: dict[str, bytes]) -> set[str]:
    found: set[str] = set()
    for name, data in files.items():
        if not name.endswith(SCAN_SUFFIXES) or name in SCAN_EXEMPT:
            continue
        found |= scan(data.decode('utf-8', 'ignore'))
    return found


def check_chunk_registry(files: dict[str, bytes]) -> None:
    """The resolver check for the two prebuilt chunks.

    A chunk's `require()`s are served by the split loader, not by node: they
    must be `~system/*` (passed through to the host) or a key some chunk has
    already published. The core chunk is evaluated first and so may only use
    `~system/*`; the smart chunk is layered on it and may also use any core key.
    `registry.json` is written by `vendor-chunks` from the same
    `src/split.rs` lists the chunks were built against, so this cannot drift
    from what the loader will actually hold.
    """
    registry = json.loads(files[CHUNK_REGISTRY])
    for rel, allowed in ((CORE_CHUNK, set()), (SMART_CHUNK, set(registry['core']))):
        code = files[rel].decode('utf-8', 'ignore')
        specs = set(re.findall(r'require\("([^"]+)"\)', code))
        specs |= set(re.findall(r"require\('([^']+)'\)", code))
        bad = sorted(s for s in specs
                     if not s.startswith('~system/') and s not in allowed)
        if bad:
            raise SystemExit(
                f'{rel} requires specifiers no chunk publishes: {", ".join(bad)}\n'
                'Add them to REGISTRY_KEYS in src/split.rs and rebuild.')
        log(f'    {rel}: {len(specs)} requires, all served')


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--work', default=os.path.join(CRATE, 'target/base-blob'))
    ap.add_argument('--keep-work', action='store_true')
    ap.add_argument('--reuse-install', action='store_true',
                    help='skip the install and reuse --work as-is')
    ap.add_argument('--sdk-bin', default=DEFAULT_SDK_BIN,
                    help='dcl-one-sdk binary that builds the prebuilt chunks')
    args = ap.parse_args()

    with open(SCAFFOLD) as fh:
        pins = json.load(fh)['devDependencies']
    install = [f'{k}@{v}' for k, v in sorted(pins.items())] + EXTRA_INSTALL
    log(f'scaffold pins: {", ".join(f"{k} {v}" for k, v in sorted(pins.items()))}')

    nm = os.path.join(args.work, 'node_modules')
    if not args.reuse_install:
        shutil.rmtree(args.work, ignore_errors=True)
        os.makedirs(args.work)
        with open(os.path.join(args.work, 'package.json'), 'w') as f:
            json.dump({'name': 'base-blob', 'version': '1.0.0', 'private': True}, f)
        log('installing ' + ' '.join(install))
        r = subprocess.run(
            ['corepack', 'pnpm', 'add', '--ignore-scripts',
             # An .npmrc with node-linker=hoisted is silently ignored by
             # pnpm 11; only the flag form flattens the tree.
             '--config.node-linker=hoisted', *install],
            cwd=args.work, capture_output=True, text=True)
        if r.returncode != 0 and 'ERR_PNPM_IGNORED_BUILDS' not in (r.stdout + r.stderr):
            sys.stderr.write(f'install failed:\n{r.stdout}\n{r.stderr}\n')
            return 1

    present = set(list_packages(nm))
    keep, why = reachable(nm, present)
    dropped = sorted(present - keep)

    log(f'\n{len(present)} packages installed -> keeping {len(keep)}')
    for p in sorted(keep):
        log(f'    keep  {p:28s} {why[p]}')
    log(f'  dropping {len(dropped)}: {", ".join(dropped)}')

    files, kept_bytes = collect(nm, keep)
    add_pbmin(files, kept_bytes)
    add_shim(files, kept_bytes)
    build_service_descriptor(args.work, files, kept_bytes)
    patch_ecs7_tsconfig(files)

    # Both of these read the UNPRUNED install tree: they are what makes the
    # pruning above safe, so they have to run before anything is thrown away.
    # The swap must land between the install and the chunk build — it rewrites a
    # file rolldown is about to resolve — and it deliberately does NOT run under
    # --reuse-install-only conditions any differently: it is idempotent, since
    # the entry it writes re-exports a sibling rather than itself.
    swap_pbmin_into_tree(args.work)
    build_chunks(args.work, args.sdk_bin, files, kept_bytes)
    check_chunk_pbmin(files)
    rollup = build_types_rollup(nm)
    files[TYPES_ROLLUP] = rollup
    kept_bytes['@dcl/js-runtime'] = kept_bytes.get('@dcl/js-runtime', 0) + len(rollup)

    specs = specifiers_in_files(files)
    unresolved = sorted(s for s in specs if not resolvable(s, set(files)))
    if unresolved:
        sys.stderr.write(
            'these imports would not resolve in the extracted tree:\n  '
            + '\n  '.join(unresolved) + '\n')
        return 1
    log(f'\nresolver check: all {len(specs)} static imports resolve')
    check_chunk_registry(files)

    with zipfile.ZipFile(OUT, 'w') as z:
        for name in sorted(files):
            info = zipfile.ZipInfo(name, date_time=FIXED_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, files[name])

    raw = sum(len(v) for v in files.values())
    log('\nper-package unpacked size:')
    for p, n in sorted(kept_bytes.items(), key=lambda kv: -kv[1]):
        log(f'  {n/1048576:8.2f} MB  {p}')
    log(f'\n{len(files)} files, {raw/1048576:.1f} MB unpacked')
    log(f'{OUT} -> {os.path.getsize(OUT)/1048576:.1f} MB')

    if not args.keep_work:
        shutil.rmtree(args.work, ignore_errors=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
