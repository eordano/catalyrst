"""Every rewrite `build-base-blob.py` applies on top of the registry install.

Imported by that script, never run. A rewrite is anything the blob carries
that a plain `pnpm add` of the scaffold pins would not produce, and every one
lives here so `src/vendor/README.md` can name them from a single place:

* `add_pbmin()` - `node_modules/protobufjs` is our dependency-free
  `protobufjs/minimal`, not upstream's; `swap_pbmin_into_tree()` points the
  install tree at the same code so the prebuilt chunks bundle it, and
  `check_chunk_pbmin()` fails the build if they ever stop doing so.
* `add_shim()` - `node_modules/@dcl/inspector` is the hand-authored stand-in
  from `src/vendor/inspector-shim` (crdt dumper + minimal data-layer host);
  `build_service_descriptor()` transpiles its rpc service descriptor beside it.
* `patch_ecs7_tsconfig()` - `@dcl/sdk/types/tsconfig.ecs7.json` loses the two
  options TypeScript 7 removes and moves to `moduleResolution: bundler`.
* `patch_ecs_network_delete_length()` - the one overlay on upstream `@dcl/*`
  JS: #1595's `DeleteEntityNetwork.write` length fix, applied to both
  `@dcl/ecs` builds before the chunks are bundled; `check_chunk_netdelete()`
  proves the chunk carries it.

The two built products - the prebuilt chunks and the declaration rollup - are
build steps, not rewrites of a file, and stay in `build-base-blob.py`. Each
function below carries its own evidence and, where one exists, its removal
condition.
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys

from blob_collect import CORE_CHUNK, SMART_CHUNK, log

CRATE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHIM = os.path.join(CRATE, 'src/vendor/inspector-shim')
PBMIN = os.path.join(CRATE, 'experiments/protobufjs-minimal-replacement')

PBMIN_ENTRY = b'module.exports = require("./index.js");\n'
PBMIN_MANIFEST = {
    'name': 'protobufjs',
    'version': '7.2.4-dcl-one-sdk-pbmin.1',
    'description': 'protobufjs/minimal wire codec, reimplemented dependency-free '
                   'for dcl-one-sdk. NOT upstream protobufjs. See '
                   'experiments/protobufjs-minimal-replacement.',
    'main': 'index.js',
    'license': 'BSD-3-Clause',
}

PBMIN_TREE_ENTRY = b'''// Redirected by scripts/blob_overlays.py swap_pbmin_into_tree().
// NOT upstream's minimal entry point. See ../protobufjs/pbmin.js.
"use strict";
module.exports = require("./pbmin.js");
'''

def swap_pbmin_into_tree(work: str) -> None:
    """Point the INSTALL TREE's `protobufjs/minimal` at the replacement too.

    `add_pbmin()` decides what the blob *ships*; this decides what the prebuilt
    chunks are *built against*. They used to disagree: rolldown resolved
    `protobufjs/minimal` through this tree and bundled UPSTREAM into
    `prebuilt/core.js`, so an extracted scene carried two protobuf codecs - ours
    in `node_modules`, upstream's inside the chunk - and the scene runtime, the
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
            f'{entry} does not exist - the install tree has no protobufjs to '
            'redirect. The package must stay in EXTRA_INSTALL/the scaffold pins '
            'even though nothing of it ships; see PROTOBUFJS_SHIP_NOTHING in '
            'blob_collect.py.')
    with open(os.path.join(PBMIN, 'index.js'), 'rb') as fh:
        core = fh.read()
    with open(os.path.join(pkg, 'pbmin.js'), 'wb') as fh:
        fh.write(core)
    with open(entry, 'wb') as fh:
        fh.write(PBMIN_TREE_ENTRY)
    log(f'    install tree: protobufjs/minimal.js -> pbmin.js  {len(core)} B '
        '(prebuilt chunks now bundle the replacement)')

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
            f'{CORE_CHUNK} does not contain {PBMIN_MARKER.decode()} - the scene '
            'runtime is bundling a protobuf codec that is not the replacement.\n'
            'swap_pbmin_into_tree() must run against the same tree build_chunks() '
            'resolves against, and before it.')

    unmangled = [n for n in (b'__getOwnPropNames', b'__defProp', b'__hasOwnProp')
                 if n in chunk]
    if unmangled:
        raise SystemExit(
            f'{CORE_CHUNK} still has unmangled top-level names '
            f'({", ".join(n.decode() for n in unmangled)}), so rolldown gave up on '
            'renaming the whole bundle.\nAlmost always a direct eval() reintroduced '
            'into a bundled module - check inquire() in the pbmin source. Expect '
            'roughly +20% on this chunk while it lasts.')
    log(f'  chunk codec check: {CORE_CHUNK} contains the pbmin marker, '
        'top-level names mangled')

def add_pbmin(files: dict[str, bytes], kept_bytes: dict[str, int]) -> None:
    """Ship OUR `protobufjs/minimal` instead of upstream's.

    `node_modules/protobufjs/{package.json,index.js,minimal.js}`, where
    `index.js` is `experiments/protobufjs-minimal-replacement/index.js`
    verbatim and `minimal.js` is a one-line re-export - the same shape as
    upstream's own `minimal.js`, so `require('protobufjs/minimal')` resolves to
    a file with the same name and no `exports` map is introduced.

    **What this replaces.** 13 files / 58,872 B of upstream `protobufjs` plus
    six whole `@protobufjs/*` micro-packages (`aspromise`, `base64`,
    `eventemitter`, `float`, `inquire`, `pool` - 25 files / 41,117 B) that only
    it imported. 38 files / 99,989 B out, 4 files / 48,340 B in - net -34 files,
    -51,649 B unpacked, -25,873 B zipped (447 -> 413 files, 2,429,940 ->
    2,404,067 B).

    **Who reads it.** Three consumers in NODE:

    * `@dcl/ecs/dist-cjs` - 69 files, the CRDT wire format `data_layer.rs`
      regenerates `main.crdt` through;
    * `@dcl/rpc` - 6 files, the data-layer socket's framing;
    * `@dcl/inspector/data-layer.gen.js` - the 22-method service descriptor
      `build_service_descriptor()` emits.

    And, since `swap_pbmin_into_tree()`, the SCENE runtime as well: the same
    code is bundled into `prebuilt/core.js`, so QuickJS runs it too. That was
    once the argument for this being low-risk - it isn't any more, and the
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
    kept_bytes.pop('protobufjs', None)
    total = 0
    src = os.path.join(PBMIN, 'index.js')
    with open(src, 'rb') as fh:
        core = fh.read()
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

SHIM_SOURCE_ONLY = ('.ts', '.proto')
SHIM_SOURCE_ONLY_NAMES = ('tsconfig.json',)
SHIM_GEN_TS = 'data-layer.gen.ts'
SHIM_GEN_JS = 'node_modules/@dcl/inspector/data-layer.gen.js'

def add_shim(files: dict[str, bytes], kept_bytes: dict[str, int]) -> None:
    """The `@dcl/inspector` stand-in: crdt dumper + minimal data-layer host.

    `dump-crdt` in `data-layer-host.mjs` needs `dumpEngineToCrdtCommands`, and
    its `serve` mode needs `createDataLayerHost` + `DataServiceDefinition`. The
    real package is 119 MB - ~90% browser editor UI - and ships separately as
    `inspector.zip`. Without this, every `.composite` scene fails build step 4
    with `Cannot find module '@dcl/inspector'`, and `start --data-layer` has no
    host to spawn.

    Three of the files copied here are generated, not written: see
    `scripts/dump-inspector-tables.cjs` for `component-schemas.json`,
    `minimal-composite.json` and `root-components.json`. All are version-pinned
    snapshots of a real `@dcl/inspector` and go stale silently when upstream
    adds a component or seeds new root state - re-run that script on every
    inspector bump.
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
    `protoc-gen-dcl_ts_proto` - a plugin from **`@dcl/ts-proto`, a DCL FORK**,
    not the `ts-proto` on npm - under
    `esModuleInterop=true,returnObservable=false,
    outputServices=generic-definitions,fileSuffix=.gen,oneof=unions,
    useMapType=true`.

    We vendor the OUTPUT (`src/vendor/inspector-shim/data-layer.gen.ts`,
    75,703 B, checked in verbatim beside the `.proto` it came from) and
    transpile it here, rather than reproducing the toolchain. Reproducing it
    would mean vendoring a ~5 MB Node codegen plugin plus `ts-proto-descriptors`
    / `case-anything` / `dprint-node` to regenerate a file that changes about
    once a year - the proto's last edit is 2025-11-13 (creator-hub 36c4130).

    Hand-writing the descriptor instead is technically possible (`codegen.js`
    reads only those five fields per method) but would be 22 hand-maintained
    message codecs matching a fork's exact wire choices, for no size win over
    83 KB.

    The transpile has no dependencies of its own: the generated file's ONLY
    imports are `long` and `protobufjs/minimal`, both already in the blob.
    `--noCheck` is what makes this a *transpile* - the two imports resolve to
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
    n = data.count(b'requestStream:')
    if n != 22:
        raise SystemExit(f'{SHIM_GEN_JS}: expected 22 methods, found {n}')
    files[SHIM_GEN_JS] = data
    kept_bytes['@dcl/inspector'] = kept_bytes.get('@dcl/inspector', 0) + len(data)
    log(f'    {SHIM_GEN_JS}  {len(data)} B, {n} methods')

ECS7_TSCONFIG = 'node_modules/@dcl/sdk/types/tsconfig.ecs7.json'
ECS7_EDITS = (
    ('downlevelIteration',
     re.compile(r'[ \t]*"downlevelIteration"\s*:\s*true,\n'), '',
     lambda opts: 'downlevelIteration' not in opts),
    ('suppressExcessPropertyErrors',
     re.compile(r'[ \t]*"suppressExcessPropertyErrors"\s*:\s*false,\n'), '',
     lambda opts: 'suppressExcessPropertyErrors' not in opts),
    ('moduleResolution',
     re.compile(r'"moduleResolution"\s*:\s*"node"'), '"moduleResolution": "bundler"',
     lambda opts: opts.get('moduleResolution') == 'bundler'),
)
ECS7_DOCS = ('src/vendor/README.md', 'docs/ts7-migration.md',
             'docs/upstream/tsconfig.md')
ECS7_SHIPPED = (
    f'{ECS7_TSCONFIG} is already TypeScript 7 clean upstream: delete '
    'patch_ecs7_tsconfig(), ECS7_EDITS and the lines naming them in '
    f'{", ".join(ECS7_DOCS)}.')

def patch_ecs7_tsconfig(files: dict[str, bytes]) -> None:
    """Make the SDK's shared tsconfig TypeScript 6/7 clean.

    EVERY scene extends this file - ours and third-party alike - so a
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
    Each of the three edits in `ECS7_EDITS` must match exactly once. An edit
    that finds nothing on a file that already reads the way it would leave it
    is that part of the upstream fix having shipped, and the build fails
    naming the edit to drop; once all three find nothing the failure says to
    delete this function - the same removal contract as
    `patch_ecs_network_delete_length()`, so the overlay cannot idle
    unnoticed, not even partially. Any other match count means upstream
    changed the option's shape, and the build fails asking for that edit to
    be re-derived from the vendored file.
    """
    raw = files[ECS7_TSCONFIG].decode('utf-8')
    before = json.loads(raw)['compilerOptions']
    out = raw

    shipped = []
    for option, pattern, replacement, upstream_clean in ECS7_EDITS:
        out, n = pattern.subn(replacement, out)
        if n == 1:
            continue
        if n == 0 and upstream_clean(before):
            shipped.append(option)
            continue
        raise SystemExit(
            f'{ECS7_TSCONFIG}: the {option} edit matched {n} times, not once, on '
            'a file that does not already read the way it would leave it, so '
            'upstream changed the shape of that option. Re-derive its ECS7_EDITS '
            'entry from the vendored file.')
    if len(shipped) == len(ECS7_EDITS):
        raise SystemExit(ECS7_SHIPPED)
    if shipped:
        edits, them = ('edit', 'it') if len(shipped) == 1 else ('edits', 'them')
        raise SystemExit(
            f'{ECS7_TSCONFIG}: upstream already carries the '
            f'{", ".join(shipped)} {edits}; drop {them} from ECS7_EDITS and '
            f'from the docs naming {them} ({", ".join(ECS7_DOCS)}) and keep '
            'the rest of the overlay.')

    after = json.loads(out)['compilerOptions']
    for gone in ('downlevelIteration', 'suppressExcessPropertyErrors'):
        if gone in after:
            raise SystemExit(f'{ECS7_TSCONFIG}: failed to drop {gone}')
    if after.get('moduleResolution') != 'bundler':
        raise SystemExit(f'{ECS7_TSCONFIG}: failed to set moduleResolution=bundler')
    if after.get('module') != 'esnext':
        raise SystemExit(f'{ECS7_TSCONFIG}: moduleResolution=bundler needs module=esnext')
    untouched = {'downlevelIteration', 'suppressExcessPropertyErrors', 'moduleResolution'}
    if {k: v for k, v in before.items() if k not in untouched} != {
        k: v for k, v in after.items() if k not in untouched
    }:
        raise SystemExit(f'{ECS7_TSCONFIG}: patch changed an unrelated option')

    files[ECS7_TSCONFIG] = out.encode('utf-8')

NETDELETE_REL = ('node_modules/@dcl/ecs/{build}/serialization/crdt/network/'
                 'deleteEntityNetwork.js')
NETDELETE_WRITE = {
    'dist-cjs': (
        b'buf.writeUint32(types_1.CRDT_MESSAGE_HEADER_LENGTH + 4);',
        b'buf.writeUint32(types_1.CRDT_MESSAGE_HEADER_LENGTH + '
        b'DeleteEntityNetwork.MESSAGE_HEADER_LENGTH);',
    ),
    'dist': (
        b'buf.writeUint32(CRDT_MESSAGE_HEADER_LENGTH + 4);',
        b'buf.writeUint32(CRDT_MESSAGE_HEADER_LENGTH + '
        b'DeleteEntityNetwork.MESSAGE_HEADER_LENGTH);',
    ),
}
NETDELETE_SHIPPED = 'REMOVE this overlay: upstream #1595 has shipped in this @dcl/ecs. '\
    'Delete NETDELETE_*, patch_ecs_network_delete_length(), check_chunk_netdelete() '\
    'and the table row + paragraph naming them in src/vendor/README.md.'

def netdelete_fixed(build: str, data: bytes, where: str, already_ok: bool) -> bytes:
    buggy, fixed = NETDELETE_WRITE[build]
    if data.count(buggy) == 1:
        return data.replace(buggy, fixed)
    if fixed in data:
        if already_ok:
            return data
        raise SystemExit(f'{where}: {NETDELETE_SHIPPED}')
    raise SystemExit(
        f'{where}: DeleteEntityNetwork.write carries the buggy length write '
        f'{data.count(buggy)} times and the fixed one never, so upstream changed '
        'its shape. Re-derive the overlay against `git show 5ae3ef7c` in '
        'js-sdk-toolchain, or drop it if the declared length is already the 16 '
        'the body needs.')

def patch_ecs_network_delete_length(work: str, files: dict[str, bytes],
                                    reuse_install: bool) -> None:
    """Make `DeleteEntityNetwork.write` declare the length it writes.

    Upstream 7.27.0 frames a network entity delete as `CRDT_MESSAGE_HEADER_LENGTH
    + 4` (12) and then writes an eight-byte body, entity + network id: the
    record claims to be four bytes shorter than it is. `@dcl/ecs` reads its own
    stream field by field and never notices. Our engine does not: the bevy
    fork's CRDT reader (bevy-explorer/crates/dcl/src/interface/mod.rs,
    `take_reader_exact`) frames strictly and discards the rest of the tick's
    batch from the first record that does not, so every Serverless-Multiplayer
    entity deletion a 7.x scene sends loses the tick against it. Upstream fixed
    it on 2026-09-03 (js-sdk-toolchain 5ae3ef7c, #1595) with the one-line
    `+ MESSAGE_HEADER_LENGTH`; that commit is not in any release yet, and this
    is that line, applied to the compiled output.

    Two copies have to change, because two consumers read two builds:

      * the INSTALL TREE, both `dist/` (ESM, what `build_chunks()` resolves
        through `main`, so `prebuilt/core.js` carries the fix inside the scene
        runtime) and `dist-cjs/` (so `--reuse-install` sees the same tree a
        fresh run produces);
      * the collected `files` dict, whose `dist-cjs` entry is what the blob
        ships for the node-side crdt dumper.

    The rewrite is exact-substring and must find its target exactly once, or
    the build fails: a release that carries upstream's fix has the fixed line
    already, and the failure then says to delete this overlay rather than let
    it idle. A reused install tree is the one place the fixed line is expected
    (this ran on the previous pass), so `--reuse-install` accepts it.
    `check_chunk_netdelete()` proves the chunk built from the patched tree.
    """
    for build in ('dist', 'dist-cjs'):
        path = os.path.join(work, NETDELETE_REL.format(build=build))
        if not os.path.isfile(path):
            raise SystemExit(f'{path} does not exist - the install tree has no '
                             f'@dcl/ecs {build} build to patch')
        with open(path, 'rb') as fh:
            data = fh.read()
        out = netdelete_fixed(build, data, path, already_ok=reuse_install)
        if out != data:
            with open(path, 'wb') as fh:
                fh.write(out)
            log(f'    install tree: @dcl/ecs/{build} DeleteEntityNetwork.write '
                'declares 8 + 8, not 8 + 4 (upstream #1595)')
        else:
            log(f'    install tree: @dcl/ecs/{build} DeleteEntityNetwork.write '
                'already patched (reused install)')
    rel = NETDELETE_REL.format(build='dist-cjs')
    if rel not in files:
        raise SystemExit(f'{rel} is not among the collected files; the overlay '
                         'has nothing to ship')
    files[rel] = netdelete_fixed('dist-cjs', files[rel], rel, already_ok=reuse_install)
    log(f'    shipped: {rel} declares 8 + 8')

NETDELETE_CHUNK_BUGGY = re.compile(
    rb'writeUint32\(12\),[\w$]+\.writeUint32\([\w$]+\.DELETE_ENTITY_NETWORK\)')
NETDELETE_CHUNK_FIXED = re.compile(
    rb'writeUint32\((?:16|8\+[\w$]+\.MESSAGE_HEADER_LENGTH)\),'
    rb'[\w$]+\.writeUint32\([\w$]+\.DELETE_ENTITY_NETWORK\)')

def check_chunk_netdelete(files: dict[str, bytes]) -> None:
    """Fail the build if the scene runtime still frames a network delete short.

    Same class of silent failure as `check_chunk_pbmin()`: the tree patch works
    by file overwrite, so a pipeline that reinstalls or bundles before
    `patch_ecs_network_delete_length()` runs ships the 12-byte header again with
    no error and a chunk that differs by a handful of bytes. The shipped
    `dist-cjs` entry is checked here too, so the two copies cannot disagree.
    """
    core = files.get(CORE_CHUNK)
    if core is None:
        raise SystemExit(f'{CORE_CHUNK} missing from the blob')
    if NETDELETE_CHUNK_BUGGY.search(core):
        raise SystemExit(
            f'{CORE_CHUNK} still declares a network entity delete as 12 bytes - '
            'patch_ecs_network_delete_length() must run against the tree '
            'build_chunks() resolves, and before it.')
    hit = NETDELETE_CHUNK_FIXED.search(core)
    if not hit:
        raise SystemExit(
            f'{CORE_CHUNK} carries no DeleteEntityNetwork.write in a shape this '
            'check knows; re-derive NETDELETE_CHUNK_FIXED from the chunk.')
    smart = files.get(SMART_CHUNK, b'')
    if NETDELETE_CHUNK_BUGGY.search(smart):
        raise SystemExit(f'{SMART_CHUNK} bundles its own, unpatched @dcl/ecs')
    rel = NETDELETE_REL.format(build='dist-cjs')
    buggy, fixed = NETDELETE_WRITE['dist-cjs']
    if buggy in files[rel] or fixed not in files[rel]:
        raise SystemExit(f'{rel} does not carry the fixed length write')
    log(f'  chunk framing check: {CORE_CHUNK} frames a network entity delete as '
        f'{hit.group(0).split(b",")[0].decode()}')
