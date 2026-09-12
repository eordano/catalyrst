#!/usr/bin/env bash
set -euo pipefail

VERSION=${1:-latest}
CRATE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
HOST=${DCL_ONE_SDK_INSPECTOR_HOST:-$HOME/.cache/dcl-one-sdk/inspector-host}
ECS_PIN=$(node -p "require('$CRATE/src/templates/init/scene/package.json').devDependencies['@dcl/sdk']")

rm -rf "$HOST"
mkdir -p "$HOST"
printf '{"name":"inspector-host","private":true}\n' > "$HOST/package.json"
npm_i() { (cd "$HOST" && npm install --no-audit --no-fund --ignore-scripts --save-exact "$@"); }

npm_i "@dcl/inspector@$VERSION"
INSP=$HOST/node_modules/@dcl/inspector
pinned() { node -p "const d=require('$INSP/package.json').devDependencies||{}; d['$1'] ? '$1@'+d['$1'] : '$1'"; }
npm_i "@dcl/ecs@$ECS_PIN" \
  "$(pinned @dcl/ecs-math)" "$(pinned @babylonjs/core)" "$(pinned node-fetch)" \
  "$(pinned ignore)" "$(pinned fp-future)" "$(pinned @well-known-components/pushable-channel)" \
  "$(pinned mitt)" "$(pinned ts-deepmerge)" "$(pinned long)" "$(pinned protobufjs)" \
  @protobufjs/utf8 "$(pinned ajv)" esbuild

ESBUILD=$HOST/node_modules/.bin/esbuild
bundle_in_place() {
  "$ESBUILD" "$1" --bundle --platform=node --format=cjs --target=node20 \
    --log-level=warning --outfile="$1.bundled"
  mv "$1.bundled" "$1"
}
bundle_in_place "$INSP/dist/tooling-entrypoint.js"
bundle_in_place "$HOST/node_modules/@dcl/asset-packs/dist/definitions.js"

node -e "
const insp = require('$INSP'), ap = require('$HOST/node_modules/@dcl/asset-packs')
for (const [n, v] of [['createEngineContext', insp.createEngineContext], ['dumpEngineToComposite', insp.dumpEngineToComposite], ['SceneAgeRating', insp.SceneAgeRating], ['asset-packs initComponents', ap.initComponents]])
  if (v === undefined) { console.error('install-inspector-host: missing ' + n); process.exit(1) }
console.log('install-inspector-host: @dcl/inspector ' + require('$INSP/package.json').version + ', @dcl/asset-packs ' + require('$HOST/node_modules/@dcl/asset-packs/package.json').version + ', @dcl/ecs ' + require('$HOST/node_modules/@dcl/ecs/package.json').version + ' at $HOST')
"
