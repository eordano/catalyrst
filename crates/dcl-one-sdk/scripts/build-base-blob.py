#!/usr/bin/env python3
"""Build `src/vendor/node_modules.zip` - the offline scene toolchain.

`init` extracts this blob so a scaffolded scene bundles, type-checks and
previews with no npm and no network. It is pure JS - no platform binaries - so
one blob serves linux, macOS and Windows.

The install list is *derived from the scaffold manifest*
(`src/templates/init/scene/package.json`), never hardcoded: the vendored set
has to match what a later `npm install` in a scaffolded scene would produce, or
the two fight instead of converging.

The blob does NOT ship the SDK as source. It ships the *build products* that
source was only ever used to derive:

* two prebuilt runtime chunks, `@dcl/sdk/prebuilt/{core,smart}.js`, and
* one rolled-up ambient declaration file, `@dcl/js-runtime/index.d.ts`.

**Why the chunks.** The SDK runtime chunk is scene-independent. Two scenes -
one importing three `@dcl/ecs` symbols, one importing react-ecs UI + tweens +
audio + animator + players + network - produce byte-identical
`bin/sdk-runtime.js`; it is keyed only on which `@dcl/*` packages are
installed, never on what the scene imports. So 3.64 MB of `.js` shipped for one
purpose: letting rolldown re-derive the same artifact on every build. It is
built here once instead, by `dcl-one-sdk vendor-chunks` (a hidden subcommand
over `src/prebuilt.rs`), which is the only place either chunk is ever produced.

The split into *two* chunks is what makes `@dcl/asset-packs` affordable in this
blob at all. `entrypoint.rs` inlines the smart-item script runtime whenever
`@dcl/asset-packs` merely *resolves*, which grew the single chunk from
463,133 B to 601,100 B (+137,967 B, +30%) - reproducibly, even for a scene with
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
(-139,751 B) - and neither chunk is rebuilt on any scene build.

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
entry does not resolve - which is what lets `@dcl/ecs` stay on disk as a
runtime-only `dist-cjs` tree (the node-side crdt dumper needs it) while its
types come from the rollup. Checked directly: a scene importing `@dcl/ecs`,
`@dcl/sdk`, `@dcl/sdk/math` and `@dcl/asset-packs/dist/scene-entrypoint`
type-checks clean against exactly that tree.

What gets dropped, and the evidence for each:

* The `.js` *and* `.d.ts` of `@dcl/sdk`, `@dcl/react-ecs`, `@dcl/ecs-math`,
  `react`, `react-reconciler`, `scheduler` and `@dcl/ecs/dist` - 4.55 MB
  in total. Every byte is already inside the two prebuilt chunks (runtime) or
  the rolled-up declaration file (types). `@dcl/sdk` keeps only its manifest
  and `types/*.json`: every scene's tsconfig extends
  `@dcl/sdk/types/tsconfig.ecs7.json`, and rolldown reads it too.
* `@dcl/ecs/dist-cjs` does NOT go. `data_layer.rs` regenerates `main.crdt` by
  running `templates/data-layer-host.mjs` in node, which loads the
  `@dcl/inspector` stand-in below, which requires `@dcl/ecs/dist-cjs` and
  `@dcl/ecs/dist-cjs/serialization/ByteBuffer` - the CRDT wire codec, in node,
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
  0.004 MB). See `RPC_RUNTIME` in `blob_collect.py` for the walk. `mitt` used to be dropped
  outright; it comes back because `@dcl/rpc` requires it in node, outside every
  chunk - but only the runtime file, so the rollup keeps owning its types.

**Why `@dcl/rpc` is in the blob at all.** The visual editor talks to a
*data-layer host* over a WebSocket: a 22-method RPC service carrying the
engine's CRDT stream in both directions. Upstream's host is `@dcl/inspector`'s,
and pulling that back in costs 119 MB for a package that is ~90% browser UI. So
the blob ships its own: `src/vendor/inspector-shim` grew from a single
`dumpEngineToCrdtCommands` into a minimal host - 4 of the 22 methods live
(`crdtStream`, `save`, `get`/`setInspectorPreferences`), the other 18 inert but
well-formed. It is a FALLBACK, not a takeover: `req()` in
`templates/data-layer-host.mjs` resolves the scene's own `node_modules` first,
so an installed `@dcl/inspector` still wins.

What that needed, and only that: `@dcl/rpc` (the wire protocol -
port multiplexing, framing, bidirectional streams; vendored, not
reimplemented), `mitt`, and the generated service descriptor
(`build_service_descriptor()`). The expensive half was already here: `@dcl/ecs`'s
live engine and CRDT reconciliation ship in `dist-cjs` for the crdt dumper, and
`protobufjs/minimal` + `long` ship for `dist-cjs`. Net cost ~0.28 MB unpacked.
`@well-known-components/pushable-channel` is NOT needed even though upstream's
`stream.ts` imports `AsyncQueue` from it - `@dcl/rpc/dist/push-channel` already
exports an `AsyncQueue` of the same shape.

What gets dropped, and the evidence for each (pre-existing prunes):

* `typescript/lib/typescript.js` (8.7 MB), `typescript.d.ts` (0.6 MB),
  `_tsserver.js`, `tsserverlibrary.*`, `typingsInstaller.js`, `watchGuard.js`,
  `typesMap.json`. `build.rs` runs exactly one thing -
  `node typescript/lib/tsc.js -p tsconfig.json --noEmit` - and a run of that
  under a node with `Module._resolveFilename` and the `fs` read family hooked
  resolves precisely two files inside the package: `lib/tsc.js` and the
  `lib/_tsc.js` it requires. `typescript.js` is the programmatic API, the
  `tsserver*` files are the editor language service; nothing in this crate,
  in `data-layer-host.mjs`, or in a scene loads either.
* `typescript/lib/<locale>/diagnosticMessages.generated.json` - 13 locales,
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
  scaffold's `package.json` - so `npm install` in a scaffolded scene does not
  install it either. Shipping it was the divergence, not dropping it. A scene
  that wants ethers must add it to its own `package.json`, exactly as it must
  in the npm flow. To put it back, add `'ethers'` to `EXTRA_INSTALL` below
  and to `ENTRY_PACKAGES` in `blob_collect.py`.
* `protobufjs` - ALL of it, plus six of the seven `@protobufjs/*`
  micro-packages. `add_pbmin()` writes `node_modules/protobufjs` from
  `experiments/protobufjs-minimal-replacement` instead: a dependency-free
  reimplementation of the `Reader`/`Writer`/`util.Long`/`configure` surface,
  4 files / ~48 KB where upstream's minimal closure plus its micro-packages
  were 38 files / 99,989 B. `@protobufjs/utf8` is the one that stays -
  `@dcl/ecs/dist-cjs` requires it *directly*, not through protobufjs, so the
  replacement does not subsume it. `long` stays too: `avatar_shape.gen.js` and
  `descriptor.gen.js` import it by name.

  This is a hand-written codec on the critical path of the scene protocol, so
  the bar for it is a differential suite, not a review. See `add_pbmin()` in
  `blob_overlays.py` for the numbers; the short version is 731,200 differential message instances and
  800,000 fuzz operations per seed against upstream 7.2.4 as the reference,
  across `@dcl/ecs` (both the CJS and the ESM build), `@dcl/rpc`'s framing and
  the data-layer descriptor, asserting byte-identical encodings and
  throw-parity - zero divergences over six seeds, and 13/13 on injected
  wire-format mutants.

  This DOES change the scene runtime. `swap_pbmin_into_tree()` redirects the
  install tree's `minimal.js` before `build_chunks()` resolves it, so
  `prebuilt/core.js` bundles the replacement and QuickJS runs it too.
  `check_chunk_pbmin()` fails the build if it ever stops doing so.

  It did not always. The swap first landed node-side only - `@dcl/ecs/dist-cjs`
  (69 files), `@dcl/rpc` (6), `@dcl/inspector/data-layer.gen.js` (1) - which
  left an extracted scene carrying TWO codecs: the replacement in
  `node_modules`, upstream's minified inside the chunk. That is the state this
  removes, and it is the reason to care; the 25,873 B was never the point.

  Be clear about the cost, because it is real: a hand-written codec on the
  scene protocol's critical path fails in every scene, not in one dev-machine
  process. The differential suite below is what that risk is priced against,
  and it covers `@dcl/ecs` in both its CJS and ESM builds - which is what the
  chunk bundles - not just the node path.

  Same class of divergence as the `typescript` prune below, and one step
  further: a scene that imports `protobufjs` *itself* (the reflection library,
  or `protobufjs/light`) resolved under `npm install` and never offline, and
  now the offline `protobufjs` is not upstream's at all. It must declare
  protobufjs in its own manifest to import it, which the scaffold does not, so
  npm would give it its own full copy anyway - and the shipped manifest says
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
  blob any more - not as a package (`add_pbmin()`) and no longer inside
  `prebuilt/core.js` either (`swap_pbmin_into_tree()`). What is still installed
  is its TYPES, which `build_types_rollup()` reads because `@dcl/ecs/dist`
  imports `protobufjs/minimal` without declaring it, and its runtime as the
  reference the differential suite compares against. The second is what keeps
  the version pinned: 7.2.4 is what the npm flow resolves, via `@dcl/sdk` ->
  `@dcl/sdk-commands` -> `@dcl/protocol`, whose manifest pins `"protobufjs":
  "7.2.4"` exactly. Installing 8.x would silently re-target the suite at a
  reference no scene runs, which is the one thing that would make the evidence
  behind the replacement worthless.
* `typescript` - see `TS_CEILING_NOTE` below and `src/vendor/README.md`.

Bootstrapping: the chunk build needs a `dcl-one-sdk` binary, and that binary
embeds this blob. There is no cycle - the binary that *builds* the chunks only
has to contain rolldown and `src/split.rs`, not the blob it will later ship. So
the order is: `cargo build --release` (with whatever blob is committed), run
this script, `cargo build --release` again to embed the new blob.

Layout. This file is the pipeline: the install, the chunk build, the
declaration rollup and `main()`. Two siblings are imported by name and never
run. `blob_collect.py` decides what of the registry install ships - the
entry points, the allowlists and the reachability walk that prunes the rest -
and `blob_overlays.py` is every rewrite applied on top of that install: the
pbmin codec, the `@dcl/inspector` stand-in and its service descriptor, the
ecs7 tsconfig patch and the #1595 framing fix, each with its evidence. A
rewrite lives nowhere else, which is what lets `src/vendor/README.md` list
them completely.

Usage:  python3 scripts/build-base-blob.py [--work DIR] [--keep-work]
                                           [--reuse-install] [--sdk-bin PATH]
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
sys.path.insert(0, HERE)

from blob_collect import (
    CHUNK_REGISTRY, CORE_CHUNK, FILE_ALLOWLIST, SMART_CHUNK, TYPES_ROLLUP,
    collect, list_packages, log, reachable, resolvable, specifiers_in_files,
)
from blob_overlays import (
    add_pbmin, add_shim, build_service_descriptor, check_chunk_netdelete,
    check_chunk_pbmin, patch_ecs7_tsconfig, patch_ecs_network_delete_length,
    swap_pbmin_into_tree,
)

CRATE = os.path.dirname(HERE)
WORKSPACE = os.path.dirname(os.path.dirname(CRATE))
SCAFFOLD = os.path.join(CRATE, 'src/templates/init/scene/package.json')
VENDOR_README = os.path.join(CRATE, 'src/vendor/README.md')
OUT = os.path.join(CRATE, 'src/vendor/node_modules.zip')
DEFAULT_SDK_BIN = os.path.join(WORKSPACE, 'target/release/dcl-one-sdk')
FIXED_DATE = (1980, 1, 1, 0, 0, 0)

TS_CEILING_NOTE = """typescript 7.x is the Go port: its `lib/getExePath.js` resolves a
per-platform native package (`@typescript/typescript-darwin-arm64` and friends),
which would make this blob platform-specific. 6.x is the ceiling until that is
addressed."""

EXTRA_INSTALL = [
    'protobufjs@7.2.4',
    '@protobufjs/utf8',
    'ws',
    '@dcl/rpc',
    '@dcl/asset-packs',
]

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
    `node_modules` tree - which is what lets the packages themselves go.

    `@dcl/js-runtime`'s own three declarations are ambient already (global
    interfaces + `declare module '~system/*'` + `declare module '~sdk/*'`) and
    are copied verbatim at top level. That is load-bearing: the bundle then has
    no top-level `import`/`export` and stays a *script*, which is what makes the
    inner `declare module` blocks ambient declarations rather than
    augmentations of modules that no longer exist.

    The keep-set is tsc reachability, not "every `.d.ts` in the package": BFS
    from the public entry points, following declaration imports the way tsc
    does. Shipping the unreachable ones would *add* errors the node_modules tree
    never produced - `@dcl/react-ecs/dist/reconciler/types.d.ts` names the DOM
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
    * `@dcl/js-runtime/apis.d.ts` reaches @dcl/ecs as `import('../ecs')` - it
      walks out of its own package directory, which only resolves while the two
      are siblings under `node_modules/@dcl/`, exactly what this bundle
      removes. Rewritten to `import('@dcl/ecs')`.
    * A relative specifier written with a `.js` extension (long's `./types.js`)
      resolves to the sibling `.d.ts`.
    * `@dcl/sdk/src/<x>` is aliased onto `@dcl/sdk/<x>`. `@dcl/sdk` ships its
      own TypeScript *sources* under `src/` and real scenes import them -
      `@dcl/sdk/src/players` and `@dcl/sdk/src/network`, two scenes in
      decentraland/sdk7-test-scenes. Those `.ts` carry implementations and
      cannot become `declare module` bodies. With the 33 alias blocks (~3 KB)
      the corpus is 60/60; without them, 58/60.
    """
    known: dict[str, str] = {}

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
    `@dcl/sdk-commands/dist/logic/runtime-script.js` - the real
    `~sdk/script-utils` runtime, from a package `NEVER` keeps.

    `vendor-chunks` verifies both chunks before returning: every `require()`
    either chunk emits must be `~system/*` or a key the loader's registry will
    hold when that chunk is evaluated. That check is what caught
    `@dcl/sdk/platform` and `@dcl/sdk/text-codec` - required by asset-packs,
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

COUNT_WORDS = ('zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven',
               'eight', 'nine', 'ten', 'eleven', 'twelve', 'thirteen',
               'fourteen', 'fifteen', 'sixteen', 'seventeen', 'eighteen',
               'nineteen', 'twenty')
README_ALLOWLIST = re.compile(
    r'(\w+)\s+packages\s+are\s+allowlisted\s+file\s+by\s+file\s+'
    r'\(`FILE_ALLOWLIST`:\s*([^)]*)\)')

def count_word(n: int) -> str:
    """Spell the allowlist count the way the README writes it.

    Bounded on purpose: a count past the table is a build failure that names
    the fix, never an IndexError from inside the drift check.
    """
    if n < len(COUNT_WORDS):
        return COUNT_WORDS[n]
    raise SystemExit(
        f'blob_collect.FILE_ALLOWLIST has {n} entries but COUNT_WORDS in '
        f'scripts/build-base-blob.py only spells 0..{len(COUNT_WORDS) - 1}; '
        'extend COUNT_WORDS so check_readme_allowlist() can spell the count '
        'the README must carry.')

def check_readme_allowlist() -> None:
    """Fail before the install if the README's allowlist sentence has drifted.

    `src/vendor/README.md` step 3 counts and names the packages in
    `FILE_ALLOWLIST`; the count went stale once already (six written, eight
    real), so the sentence is checked against the dict instead of trusted.
    """
    with open(VENDOR_README, encoding='utf-8') as fh:
        hit = README_ALLOWLIST.search(fh.read())
    if not hit:
        raise SystemExit(f'{VENDOR_README}: step 3 no longer carries the '
                         '"<count> packages are allowlisted file by file '
                         '(`FILE_ALLOWLIST`: ...)" sentence this check reads')
    named = set(re.findall(r'`([^`]+)`', hit.group(2)))
    real = set(FILE_ALLOWLIST)
    want = count_word(len(real))
    if named != real or hit.group(1) != want:
        raise SystemExit(
            f'{VENDOR_README}: step 3 names {hit.group(1)} allowlisted packages '
            f'({", ".join(sorted(named))}); blob_collect.FILE_ALLOWLIST has '
            f'{want} ({", ".join(sorted(real))}). Fix the README.')

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--work', default=os.path.join(CRATE, 'target/base-blob'))
    ap.add_argument('--keep-work', action='store_true')
    ap.add_argument('--reuse-install', action='store_true',
                    help='skip the install and reuse --work as-is')
    ap.add_argument('--sdk-bin', default=DEFAULT_SDK_BIN,
                    help='dcl-one-sdk binary that builds the prebuilt chunks')
    args = ap.parse_args()
    check_readme_allowlist()

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

    swap_pbmin_into_tree(args.work)
    patch_ecs_network_delete_length(args.work, files, args.reuse_install)
    build_chunks(args.work, args.sdk_bin, files, kept_bytes)
    check_chunk_pbmin(files)
    check_chunk_netdelete(files)
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
