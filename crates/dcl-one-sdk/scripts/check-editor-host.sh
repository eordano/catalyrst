#!/usr/bin/env bash
# Type check the editor's data-layer host (src/vendor/inspector-shim).
#
# The shim is the `@dcl/inspector` stand-in the editor talks to. It has to stay
# runnable JavaScript -- build-base-blob.py copies these files into the blob's
# node_modules/@dcl/inspector verbatim -- so it is typed with JSDoc and checked
# with `checkJs` rather than ported to TypeScript.
#
# The types cannot come from the blob beside it: the blob ships @dcl/ecs and
# @dcl/rpc pruned of every .d.ts (236 and 14 files, zero declarations), which is
# correct for something a scene unpacks to RUN and useless for checking. So the
# check borrows a real install.
#
#   DCL_ONE_SDK_SHIM_TYPES=/path/to/scene/node_modules  explicit
#   otherwise                                            the first scene
#                                                        node_modules under the
#                                                        repo with @dcl/ecs,
#                                                        @dcl/rpc and typescript
#
# Skips with 0 when none is reachable: a machine with no scene installed cannot
# run this, and a check that cannot run must not read as a failure. It says so
# on stderr rather than passing quietly.
set -uo pipefail

SHIM="$(cd "$(dirname "${BASH_SOURCE[0]}")/../src/vendor/inspector-shim" && pwd)"
REPO="$(cd "$SHIM/../../../../.." && pwd)"

have_types() {
    [ -d "$1/@dcl/ecs" ] && [ -d "$1/@dcl/rpc" ] && [ -x "$1/typescript/bin/tsc" ] &&
        [ -n "$(find "$1/@dcl/ecs" -name '*.d.ts' -print -quit 2>/dev/null)" ]
}

NM="${DCL_ONE_SDK_SHIM_TYPES:-}"
if [ -n "$NM" ]; then
    have_types "$NM" || {
        echo "check-editor-host: DCL_ONE_SDK_SHIM_TYPES=$NM lacks @dcl/ecs types, @dcl/rpc or typescript" >&2
        exit 1
    }
else
    while IFS= read -r candidate; do
        if have_types "$candidate"; then NM="$candidate"; break; fi
    done < <(find "$REPO" -maxdepth 5 -type d -name node_modules -not -path '*/node_modules/*' 2>/dev/null)
fi

if [ -z "$NM" ]; then
    echo "check-editor-host: SKIPPED -- no scene node_modules with @dcl/ecs types found." >&2
    echo "  set DCL_ONE_SDK_SHIM_TYPES=/path/to/scene/node_modules to run it" >&2
    exit 0
fi

# tsc resolves bare imports by walking up from the file, and the shim lives in
# the Rust crate rather than under a node_modules. Check a copy that has one,
# so nothing is written next to the source and the source needs no symlink.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp "$SHIM"/*.js "$SHIM"/*.json "$SHIM"/data-layer.gen.ts "$WORK/" || exit 1
ln -s "$NM" "$WORK/node_modules"

echo "check-editor-host: types from $NM"
"$NM/typescript/bin/tsc" -p "$WORK/tsconfig.json" --pretty false || exit 1
echo "check-editor-host: types OK"

# The descriptor ships transpiled (build-base-blob.py emits data-layer.gen.js),
# so the runtime half needs the same .js the blob would carry.
"$NM/typescript/bin/tsc" "$WORK/data-layer.gen.ts" \
    --target ES2022 --module commonjs --esModuleInterop --skipLibCheck \
    --outDir "$WORK" --pretty false >/dev/null 2>&1
[ -f "$WORK/data-layer.gen.js" ] || {
    echo "check-editor-host: could not transpile data-layer.gen.ts" >&2
    exit 1
}

NODE="$(command -v node || true)"
if [ -z "$NODE" ]; then
    echo "check-editor-host: node not on PATH -- skipping the runtime half" >&2
    exit 0
fi
"$NODE" "$(dirname "${BASH_SOURCE[0]}")/check-editor-host-runtime.cjs" "$WORK"
