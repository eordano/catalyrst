#!/usr/bin/env bash
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

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp "$SHIM"/*.js "$SHIM"/*.json "$SHIM"/data-layer.gen.ts "$WORK/" || exit 1
ln -s "$NM" "$WORK/node_modules"

echo "check-editor-host: types from $NM"
"$NM/typescript/bin/tsc" -p "$WORK/tsconfig.json" --pretty false || exit 1
echo "check-editor-host: types OK"

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
