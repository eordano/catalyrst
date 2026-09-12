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

PREBUILT_DIR = 'node_modules/@dcl/sdk/prebuilt'
CORE_CHUNK = f'{PREBUILT_DIR}/core.js'
SMART_CHUNK = f'{PREBUILT_DIR}/smart.js'
CHUNK_REGISTRY = f'{PREBUILT_DIR}/registry.json'
TYPES_ROLLUP = 'node_modules/@dcl/js-runtime/index.d.ts'

ENTRY_PACKAGES = ['@dcl/sdk', '@dcl/js-runtime', '@dcl/ecs', 'typescript',
                  'protobufjs', '@protobufjs/utf8', 'ws', '@dcl/rpc']

ENTRY_REASON = {
    '@dcl/sdk': 'types/tsconfig.ecs7.json + the prebuilt runtime chunks',
    '@dcl/js-runtime': 'scene ambient types (the rolled-up .d.ts)',
    '@dcl/ecs': 'dist-cjs: the crdt wire codec the inspector stand-in runs in node',
    'typescript': 'build --production type check',
    'protobufjs': "undeclared import of @dcl/ecs/dist-cjs ('protobufjs/minimal')"
                  ' - ships 0 upstream files, see add_pbmin()',
    '@protobufjs/utf8': 'undeclared import of @dcl/ecs/dist-cjs',
    'ws': 'data-layer-host.mjs',
    '@dcl/rpc': 'the data-layer wire protocol (server, codegen, WebSocket transport)',
}

NEVER = {'@dcl/sdk-commands', '@dcl/explorer'}

PROTOBUFJS_SHIP_NOTHING = frozenset()

RPC_RUNTIME = frozenset({
    'package.json', 'LICENSE',
    'dist/index.js', 'dist/types.js', 'dist/server.js', 'dist/client.js',
    'dist/client-request-dispatcher.js', 'dist/message-dispatcher.js',
    'dist/stream-protocol.js', 'dist/push-channel.js', 'dist/codegen.js',
    'dist/protocol/index.js', 'dist/protocol/helpers.js',
    'dist/transports/WebSocket.js',
})

FILE_ALLOWLIST = {
    'protobufjs': lambda rel: rel in PROTOBUFJS_SHIP_NOTHING,
    'typescript': lambda rel: rel in ('package.json', 'LICENSE.txt')
    or rel in ('lib/tsc.js', 'lib/_tsc.js')
    or (rel.startswith('lib/lib.') and rel.endswith('.d.ts')),
    '@dcl/sdk': lambda rel: rel in ('package.json', 'LICENSE')
    or (rel.startswith('types/') and rel.endswith('.json')),
    '@dcl/js-runtime': lambda rel: rel in ('package.json', 'LICENSE'),
    '@dcl/ecs': lambda rel: rel in ('package.json', 'LICENSE')
    or (rel.startswith('dist-cjs/')
        and not rel.endswith(DECLARATION_SUFFIXES + DROP_SUFFIXES)),
    'long': lambda rel: not rel.endswith(DECLARATION_SUFFIXES + DROP_SUFFIXES),
    '@dcl/rpc': lambda rel: rel in RPC_RUNTIME,
    'mitt': lambda rel: rel in ('package.json', 'LICENSE', 'dist/mitt.js'),
}

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

VIRTUAL_PREFIXES = ('~sdk/', '~system/')

OPTIONAL_SPECS = {'bufferutil', 'utf-8-validate'}

DROP_SUFFIXES = ('.map', '.md', '.markdown', '.flow')
DECLARATION_SUFFIXES = ('.d.ts', '.d.cts', '.d.mts')
DROP_DIRS = {
    'test', 'tests', '__tests__', 'docs', 'example', 'examples',
    'bench', 'benchmark', 'benchmarks', '.github',
}
DROP_FILES = {'eslint.config.js', '.eslintrc.js', '.eslintrc.cjs', 'karma.conf.js'}
DROP_FILE_RE = re.compile(r'(^|\.)(test|tests|spec)\.(js|cjs|mjs|ts)$')

SPEC_RE = re.compile(
    r'''(?:require\(\s*|(?:^|[\s;{}])(?:import|export)[^'"()]{0,200}?from\s*|import\(\s*)'''
    r'''['"]([^'"\n]+)['"]''',
    re.M,
)
REF_TYPES_RE = re.compile(r'///\s*<reference\s+types\s*=\s*"([^"]+)"')

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
        return True
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
    'node_modules/typescript/lib/_tsc.js',
    CORE_CHUNK,
    SMART_CHUNK,
    TYPES_ROLLUP,
}

def specifiers_in_files(files: dict[str, bytes]) -> set[str]:
    found: set[str] = set()
    for name, data in files.items():
        if not name.endswith(SCAN_SUFFIXES) or name in SCAN_EXEMPT:
            continue
        found |= scan(data.decode('utf-8', 'ignore'))
    return found
