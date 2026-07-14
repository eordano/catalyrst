"""What of the registry install ships, and where the built pieces land.

Imported by `build-base-blob.py`, never run. The evidence for every prune is
in that script's module docstring; this module is the policy it describes -
the entry points, the per-package allowlists, the drop lists - and the
reachability walk (`reachable()` -> `collect()`) that turns them into the file
set, plus the resolver check (`resolvable()`) that proves the set is closed
under its own imports. The layout constants at the top are where the two
built products land; `blob_overlays.py` reads them to verify the chunks.
"""
from __future__ import annotations

import os
import re

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

# npm-toolchain-only, ~160 MB, and only ever reached through `dependencies` -
# never through an import in the code that ships.
NEVER = {'@dcl/sdk-commands', '@dcl/explorer'}

# `protobufjs` is INSTALLED but nothing of it is SHIPPED - `add_pbmin()` writes
# `node_modules/protobufjs` instead, from
# `experiments/protobufjs-minimal-replacement`. See that function for the
# evidence. The package still has to be installed, but only for its TYPES:
# `build_types_rollup()` needs its declarations, because `@dcl/ecs/dist/*.gen.d.ts`
# import `protobufjs/minimal`. Its runtime is no longer used anywhere -
# `swap_pbmin_into_tree()` redirects `minimal.js` at the replacement before
# `build_chunks()` runs, so `prebuilt/core.js` bundles pbmin too and the whole
# toolchain has exactly one protobuf codec.
#
# Shipping nothing also has a second effect, and it is the one that pays: the
# BFS in `reachable()` walks only files `wanted()` keeps, so with protobufjs
# contributing no specifiers the six `@protobufjs/*` micro-packages it was the
# sole importer of fall out on their own - exactly the way
# `@protobufjs/{codegen,path,fetch}` already did. `@protobufjs/utf8` does NOT
# fall out and must not: `@dcl/ecs/dist-cjs` requires it directly
# (`serialization/ByteBuffer/index.js` and `components/component-number.js`),
# not through protobufjs, and it stays an entry point below.
PROTOBUFJS_SHIP_NOTHING = frozenset()

# The `@dcl/rpc` runtime closure. Three entry points are named - `@dcl/rpc`
# (`createRpcServer`), `@dcl/rpc/dist/codegen` and
# `@dcl/rpc/dist/transports/WebSocket` by `data-layer-host.mjs`, plus
# `@dcl/rpc/dist/push-channel` by the inspector stand-in's stream - and a
# `require()` walk from those closes over exactly the 12 `.js` below.
#
# The package's whole `require()` surface, across every file it ships, is
# `mitt` and `protobufjs/minimal`. `ts-proto` is in its `dependencies` and is
# never required at runtime - it is the codegen plugin, not a runtime.
#
# 66 files / 314,376 B installed -> 14 files / 110,783 B kept. What goes:
# every `.d.ts` and `.js.map` (the `dist` is mostly declarations and maps),
# `dist/rpc.api.json` (46,818 B of api-extractor report), `dist/protocol/
# index.proto`, `dist/tsdoc-metadata.json`, README. Three runtime files are
# unreachable and go too: `dist/transports/{Memory,WebWorker}.js` (the browser
# and in-process transports - we only ever attach a WebSocket) and
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
# surface. Only the files listed can ever be loaded; the resolver check
# (`resolvable()`, run last by build-base-blob.py) proves the kept set is enough.
FILE_ALLOWLIST = {
    'protobufjs': lambda rel: rel in PROTOBUFJS_SHIP_NOTHING,
    'typescript': lambda rel: rel in ('package.json', 'LICENSE.txt')
    or rel in ('lib/tsc.js', 'lib/_tsc.js')
    or (rel.startswith('lib/lib.') and rel.endswith('.d.ts')),
    # Manifest + the shared tsconfigs every scene extends, and nothing else:
    # the implementation is in `prebuilt/*.js` and the declarations are in the
    # rollup. `types/` is a directory, not a single file - 7.26.0 ships
    # `tsconfig.ecs7.json` and `tsconfig.ecs7.strict.json`, and a scene may
    # extend either (`77,-5-tweens-moving-platforms` extends the strict one).
    '@dcl/sdk': lambda rel: rel in ('package.json', 'LICENSE')
    or (rel.startswith('types/') and rel.endswith('.json')),
    # Manifest only. `index.d.ts` is injected later - it is the rollup.
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
# like a cheap 0.09 MB - but it is 0.09 MB of duplicate, and worse than
# duplicate. Its manifest's `typings` points at `dist/definitions.d.ts`, so an
# on-disk copy resolves and tsc prefers it over the ambient `declare module`
# blocks the rollup emits; and a wholesale copy reintroduces the declarations
# `build_types_rollup()`'s reachability walk deliberately excludes (the
# `admin-toolkit-ui/**` half - nothing public re-exports it), which is the same
# trap `@dcl/react-ecs/dist/reconciler/types.d.ts` sprang with its DOM
# `Document` reference. The TS7016 risk that argument is usually made from is
# already handled where it belongs: drop a `.d.ts` the closure needs and
# `build_types_rollup()` raises on unresolved relative specifiers before
# build-base-blob.py writes anything.
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
# chunk. Only `dist/mitt.js` ships - 349 B - and the `.d.ts` still does not, so
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
# Test files that are not under a test/ directory - `protobufjs/ext/descriptor/
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
    literally named `x` - `@dcl/js-runtime` is the latter.
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
    # even when a userland package of the same name happens to be installed -
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
        return True  # relative / builtin - resolved within its own package
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
    # The prebuilt chunks. Their imports are NOT resolved against node_modules -
    # the split loader resolves them against the registry the previous chunk
    # published - so `resolvable()` is the wrong question to ask of them.
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
