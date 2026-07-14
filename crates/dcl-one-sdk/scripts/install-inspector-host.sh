#!/usr/bin/env bash
# install-inspector-host.sh — a require()-able @dcl/inspector for
# scripts/dump-inspector-tables.cjs, at ~/.cache/dcl-one-sdk/inspector-host.
#
# A bare `npm install @dcl/inspector` has not been loadable since 7.37.0: the
# CommonJS entrypoint require()s ten packages it does not declare
# (docs/upstream/inspector-packaging.md), and two of the trees it needs —
# @dcl/ecs-math and @dcl/asset-packs — ship extensionless ESM imports that
# node's require(esm) cannot resolve. So this installs the package together
# with its undeclared dependencies (versions taken from the package's own
# devDependencies where it pins them), then bundles the two entrypoints the
# dump script loads — @dcl/inspector and its @dcl/asset-packs sibling — into
# self-contained CommonJS with esbuild, in place. @dcl/ecs is pinned to the
# scaffold's @dcl/sdk version (the two release in lockstep) so the dump script's
# factory probe sees the same ecs the blob ships.
#
# Usage: scripts/install-inspector-host.sh [<@dcl/inspector version>]
#          (default: latest)
# then:  node scripts/dump-inspector-tables.cjs
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
# The package pins the versions it was built against in devDependencies; take
# those where present so the bundle matches what upstream's own tree resolves.
pinned() { node -p "const d=require('$INSP/package.json').devDependencies||{}; d['$1'] ? '$1@'+d['$1'] : '$1'"; }
npm_i "@dcl/ecs@$ECS_PIN" \
  "$(pinned @dcl/ecs-math)" "$(pinned @babylonjs/core)" "$(pinned node-fetch)" \
  "$(pinned ignore)" "$(pinned fp-future)" "$(pinned @well-known-components/pushable-channel)" \
  "$(pinned mitt)" "$(pinned ts-deepmerge)" "$(pinned long)" "$(pinned protobufjs)" \
  @protobufjs/utf8 "$(pinned ajv)" esbuild
# (@protobufjs/utf8: the bundle require()s it directly; protobufjs 8 no longer
# depends on it, so it is installed on its own rather than trusted to arrive.)

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
