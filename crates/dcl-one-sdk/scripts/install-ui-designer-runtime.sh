#!/usr/bin/env bash
set -euo pipefail

inspector_dir=${1:?Usage: install-ui-designer-runtime.sh /path/to/@dcl/inspector}
inspector_dir=$(cd "$inspector_dir" && pwd)
test -f "$inspector_dir/public/bundle.js"
if ! rg -q 'uiDesignerOpen' "$inspector_dir/public/bundle.js"; then
  echo 'This inspector build does not contain the upstream UI Designer.' >&2
  exit 1
fi
runtime_tmp=$(mktemp -d)
trap 'rm -rf "$runtime_tmp"' EXIT
printf '{"name":"sdk-ui-designer-runtime","private":true}\n' > "$runtime_tmp/package.json"
(
  cd "$runtime_tmp"
  npm install --no-audit --no-fund --ignore-scripts --save-exact \
    @oxc-parser/wasm@0.60.0 @dcl/mini-rpc@1.0.7 esbuild@0.25.12
)
cat > "$runtime_tmp/runtime.mjs" <<'JS'
import init, { parseSync } from '@oxc-parser/wasm/web/oxc_parser_wasm.js';
export { RPC, Transport } from '@dcl/mini-rpc';
let ready;
export async function parse(filename, source) {
  ready ??= init({ module_or_path: new URL('./oxc_parser_wasm_bg.wasm', import.meta.url) });
  await ready;
  const result = parseSync(source, { sourceFilename: filename });
  try { return { program: result.program, comments: result.comments, errors: result.errors }; }
  finally { result.free(); }
}
JS
mkdir -p "$inspector_dir/public/sdk-ui-designer"
"$runtime_tmp/node_modules/.bin/esbuild" "$runtime_tmp/runtime.mjs" --bundle --format=esm --platform=browser \
  --outfile="$inspector_dir/public/sdk-ui-designer/runtime.js"
cp "$runtime_tmp/node_modules/@oxc-parser/wasm/web/oxc_parser_wasm_bg.wasm" "$inspector_dir/public/sdk-ui-designer/"
printf '{"oxc":"0.60.0","miniRpc":"1.0.7","uiDesigner":true}\n' > "$inspector_dir/public/sdk-ui-designer/manifest.json"
echo "UI Designer runtime installed in $inspector_dir/public/sdk-ui-designer"
