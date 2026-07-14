#!/usr/bin/env bash
# coverage-tour.sh — exercise the entire user-facing dcl-one-sdk CLI surface.
#
# Usage: coverage-tour.sh <instrumented-bin> <workdir> [case-filter-regex]
#   <instrumented-bin>  dcl-one-sdk built with:
#                       RUSTFLAGS="-C instrument-coverage \
#                         -C llvm-args=--runtime-counter-relocation \
#                         -C llvm-args=--instrprof-atomic-counter-update-all"
#                       (the llvm-args enable LLVM_PROFILE_FILE %c continuous
#                       mode, so servers killed mid-run still write profiles)
#   <workdir>           scratch dir; created; profraws land in <workdir>/profraw
#   [case-filter-regex] run only cases whose "NN name" matches (grep -E)
#
# Every case is numbered and named — the driver table at the bottom doubles as
# the case inventory for docs/coverage-tour.md. The harness, stub servers, and
# fixture writers live in the sourced coverage-tour-lib.sh beside this script.
# The script is offline: every network target is a local python stub or a dead
# loopback port (ports 61000+). All spawned processes are killed on exit.
# Exit code: number of FAILed cases (0 = all pass/skip).

set -u

BIN=${1:?usage: coverage-tour.sh <bin> <workdir> [filter]}
WORK=${2:?usage: coverage-tour.sh <bin> <workdir> [filter]}
FILTER=${3:-.}

BIN=$(readlink -f "$BIN")
mkdir -p "$WORK"/{profraw,logs}
WORK=$(readlink -f "$WORK")
LOGS=$WORK/logs
SREQ=$LOGS/stub-requests.log
LH=http://127.0.0.1
WS=ws://127.0.0.1

export LLVM_PROFILE_FILE=${LLVM_PROFILE_FILE:-$WORK/profraw/tour-%p-%m%c.profraw}
unset DCL_PRIVATE_KEY RUST_LOG NO_COLOR 2>/dev/null || true
for v in $(env | sed -n 's/^\(DCL_ONE_SDK_[A-Z_]*\)=.*/\1/p'); do unset "$v"; done
export DCL_ONE_SDK_CATALYST=$LH:61999   # dead port: no back-fill escapes

KEY=0000000000000000000000000000000000000000000000000000000000000001
ADDR=0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
STUB_PORT=61040
STUB=$LH:$STUB_PORT
DEAD=$LH:61998
NODE=$(command -v node)

SCENE=$WORK/scene
WEARABLE=$WORK/wearable
COMPOSITE_SCENE=$WORK/composite-scene
WSROOT=$WORK/ws-root
DEPLOY_MIN=$WORK/deploy-min
WORLD_MIN=$WORK/world-min
CRATE=$(cd "$(dirname "$0")/.." && pwd)
NPM_SCENE=${TOUR_NPM_SCENE:-$(cd "$(dirname "$0")/../../.." && pwd)/target/tmp/dcl-one-sdk-npm-scene}
TUNNEL_BIN=${TOUR_TUNNEL_BIN:-$(dirname "$BIN")/catalyrst-preview-tunnel}

. "$(dirname "$0")/coverage-tour-lib.sh"

c01_version() { "$BIN" --version | grep -q '^dcl-one-sdk '; }
c02_help_top() { "$BIN" --help | grep -q 'Binary-compatible Rust replacement'; }
c03_help_subcommands() {
  for sub in init get-context-files build start deploy unpublish pack world \
             "world settings" "world settings get" "world settings set" \
             "world permissions" "world permissions list" "world permissions grant" \
             "world permissions revoke"; do
    # shellcheck disable=SC2086
    "$BIN" $sub --help > /dev/null || return 1
  done
}
c04_unknown_flag() {
  fails "$BIN" build --frobnicate 2> "$LOGS/.c04"
  grep -q 'unexpected argument' "$LOGS/.c04"
}
c05_unknown_subcommand() { ! "$BIN" frobnicate > /dev/null 2>&1; }
c06_no_args() { ! "$BIN" > /dev/null 2>&1; }

c07_init_scene() {
  rm -rf "$SCENE"
  "$BIN" init --dir "$SCENE" --project scene
  test -f "$SCENE/scene.json" && test -d "$SCENE/node_modules/@dcl/sdk" \
    && test -f "$SCENE/package.json" && test -f "$SCENE/tsconfig.json"
}
c08_init_refuses_non_empty() {
  fails "$BIN" init --dir "$SCENE" 2> "$LOGS/.c08"
  grep -qi 'not empty' "$LOGS/.c08"
}
c09_init_yes_non_empty() {
  local d=$WORK/init-yes; rm -rf "$d"; mkdir -p "$d"; touch "$d/existing.txt"
  "$BIN" init --dir "$d" -y --project scene
  test -f "$d/scene.json" && test -f "$d/existing.txt"
}
c10_init_smart_wearable() {
  rm -rf "$WEARABLE"
  "$BIN" init --dir "$WEARABLE" --project smart-wearable
  test -f "$WEARABLE/wearable.json"
  grep -q 'pack' "$WEARABLE/package.json"
}
c11_init_node_modules_only() {
  local d=$WORK/init-nm; rm -rf "$d"; mkdir -p "$d"
  cp "$SCENE/scene.json" "$SCENE/package.json" "$d/"
  "$BIN" init --dir "$d" --node-modules-only
  test -d "$d/node_modules/@dcl/sdk"
}
c12_init_non_tty_defaults_scene() {
  local d=$WORK/init-notty; rm -rf "$d"
  "$BIN" init --dir "$d" < /dev/null
  test -f "$d/scene.json" && ! test -f "$d/wearable.json"
}
c13_init_tty_prompt_wearable() {
  need_pty
  local d=$WORK/init-pty; rm -rf "$d"
  printf '2\n' | script -qec "$BIN init --dir $d" /dev/null > "$LOGS/.c13" || true
  test -f "$d/wearable.json"
}
c14_init_tty_prompt_bad_answer() {
  need_pty
  local d=$WORK/init-pty-bad; rm -rf "$d"
  printf 'zebra\n' | script -qec "$BIN init --dir $d" /dev/null > "$LOGS/.c14" || true
  grep -qi 'scene\|wearable' "$LOGS/.c14" && ! test -f "$d/scene.json"
}
c15_init_target_is_file() {
  local f=$WORK/init-file.txt; touch "$f"
  fails "$BIN" init --dir "$f" 2> "$LOGS/.c15"
  grep -qi 'file' "$LOGS/.c15"
}

c16_ctx_offline_in_project() {
  "$BIN" get-context-files --dir "$SCENE" --offline
  test -f "$SCENE/.claude/skills/migrate-smart-items-to-code/SKILL.md"
}
c17_ctx_outside_project() {
  local d=$WORK/not-a-project; mkdir -p "$d"
  "$BIN" get-context-files --dir "$d" > "$LOGS/.c17" 2>&1
  fails test -d "$d/.claude" && ! test -d "$d/dclcontext"
}
c18_ctx_download_from_stub() {
  mkdir -p "$SCENE/dclcontext"; echo old > "$SCENE/dclcontext/stale.md"
  DCL_ONE_SDK_CONTEXT_API=$STUB/api/root "$BIN" get-context-files --dir "$SCENE"
  grep -q alpha "$SCENE/dclcontext/a.md" && grep -q beta "$SCENE/dclcontext/b.md" \
    && ! test -f "$SCENE/dclcontext/stale.md"
}
c19_ctx_unreachable_api() {
  DCL_ONE_SDK_CONTEXT_API=$DEAD/api "$BIN" get-context-files --dir "$SCENE" > "$LOGS/.c19" 2>&1
  grep -qi 'skill' "$LOGS/.c19"
}
c20_ctx_partial_failure() {
  DCL_ONE_SDK_CONTEXT_API=$STUB/api/partial "$BIN" get-context-files --dir "$SCENE" > "$LOGS/.c20" 2>&1
  grep -q '1 successful, 1 failed' "$LOGS/.c20"
}

c21_build_dev() {
  "$BIN" build --dir "$SCENE" | grep -q 'Type check passed'
  test -s "$SCENE/bin/index.js" && test -s "$SCENE/bin/scene.js" && test -s "$SCENE/bin/sdk-runtime.js"
}
c22_build_production() { "$BIN" build --dir "$SCENE" --production; }
c23_build_skip_type_check() { "$BIN" build --dir "$SCENE" --skip-type-check; }
c24_build_custom_entry_point() { "$BIN" build --dir "$SCENE" --customEntryPoint --skip-type-check; }
c25_build_composite_scene() {
  rm -rf "$COMPOSITE_SCENE"
  "$BIN" init --dir "$COMPOSITE_SCENE" --project scene
  write_composite "$COMPOSITE_SCENE/assets/scene/main.composite"
  "$BIN" build --dir "$COMPOSITE_SCENE" --skip-type-check > "$LOGS/.c25" 2>&1
  test -f "$COMPOSITE_SCENE/main.crdt"
}
c26_build_ignore_composite() {
  rm -f "$COMPOSITE_SCENE/main.crdt"
  "$BIN" build --dir "$COMPOSITE_SCENE" --ignoreComposite --skip-type-check
  fails test -f "$COMPOSITE_SCENE/main.crdt"
}
c27_build_workspace() {
  rm -rf "$WSROOT"; mkdir -p "$WSROOT"
  printf '{"folders":[{"path":"member-a"},{"path":"member-b"}]}' > "$WSROOT/dcl-workspace.json"
  for m in member-a member-b; do
    mkdir -p "$WSROOT/$m/src"
    cp "$SCENE/tsconfig.json" "$WSROOT/$m/"
    echo 'export function main() {}' > "$WSROOT/$m/src/index.ts"
    cp -al "$SCENE/node_modules" "$WSROOT/$m/node_modules"
  done
  printf '{"display":{"title":"A"},"main":"bin/index.js","runtimeVersion":"7","scene":{"parcels":["60,60"],"base":"60,60"}}' > "$WSROOT/member-a/scene.json"
  printf '{"display":{"title":"B"},"main":"bin/index.js","runtimeVersion":"7","scene":{"parcels":["61,60"],"base":"61,60"}}' > "$WSROOT/member-b/scene.json"
  "$BIN" build --dir "$WSROOT" --skip-type-check | grep -q 'in member-b'
}
c28_build_syntax_error() {
  local d=$WORK/broken-syntax
  broken_src "$d" 'export function main() { const x = = 1 }'
  fails "$BIN" build --dir "$d" 2> "$LOGS/.c28"
  grep -q 'build failed' "$LOGS/.c28"
}
c29_build_type_error() {
  local d=$WORK/broken-types
  broken_src "$d" "export function main() { const x: number = 'nope'; return x }"
  fails "$BIN" build --dir "$d" 2> "$LOGS/.c29"
  grep -q 'type check failed' "$LOGS/.c29" && grep -q 'skip-type-check' "$LOGS/.c29"
}
c30_build_broken_composite() {
  local d=$WORK/broken-composite; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  mkdir -p "$d/assets/scene"
  echo '{ not json' > "$d/assets/scene/main.composite"
  "$BIN" build --dir "$d" --skip-type-check > "$LOGS/.c30" 2>&1 || true
  grep -qi 'composite' "$LOGS/.c30"
}
c31_build_oversize_composite() {
  local d=$WORK/oversize-composite; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  mkdir -p "$d/assets/scene"
  python3 -c "open('$d/assets/scene/main.composite','w').write('{\"version\":1,\"components\":[' + ' ' * 17000000 + ']}')"
  "$BIN" build --dir "$d" --skip-type-check > "$LOGS/.c31" 2>&1
  grep -q 'main.crdt skipped (no composite)' "$LOGS/.c31"   # >16MiB refused = excluded, not fatal
}
c32_build_watch_edit_rebuilds() {
  local d=$WORK/watch-scene; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  spawn "$BIN" build --dir "$d" --watch --skip-type-check
  local wpid=$LAST_PID log=$LOGS/32-build_watch_edit_rebuilds.log
  wait_for 90 grep -q 'Watching for changes' "$log" 2>/dev/null
  echo 'export function main() { console.log(2) }' > "$d/src/index.ts"
  wait_for 60 grep -q 'rebuilt' "$log" 2>/dev/null
  kill -INT "$wpid"; sleep 1; kill -9 "$wpid" 2>/dev/null || true
}
c33_build_watch_workspace_rebuilds() {
  spawn "$BIN" build --dir "$WSROOT" --watch --skip-type-check
  local wpid=$LAST_PID log=$LOGS/33-build_watch_workspace_rebuilds.log
  wait_for 90 grep -q 'Watching for changes' "$log" 2>/dev/null
  echo 'export function main() { console.log(5) }' > "$WSROOT/member-a/src/index.ts"
  wait_for 60 grep -q 'rebuilt' "$log" 2>/dev/null
  kill -INT "$wpid"; sleep 1; kill -9 "$wpid" 2>/dev/null || true
}
c34_build_missing_tsconfig() {
  local d=$WORK/no-tsconfig; rm -rf "$d"; mkdir -p "$d/src"
  cp "$SCENE/scene.json" "$d/"
  ln -s "$SCENE/node_modules" "$d/node_modules"
  echo 'export function main() {}' > "$d/src/index.ts"
  fails "$BIN" build --dir "$d" 2> "$LOGS/.c34"
  grep -q 'tsconfig' "$LOGS/.c34"
}
c35_build_workspace_broken_member() {
  local d=$WORK/ws-broken; rm -rf "$d"; mkdir -p "$d"
  printf '{"folders":[{"path":"missing-member"}]}' > "$d/dcl-workspace.json"
  fails "$BIN" build --dir "$d" 2> "$LOGS/.c35"
  grep -q 'dcl-workspace.json' "$LOGS/.c35"
}

c36_start_probe_suite() {
  STARTDIR=$SCENE start_static 61010 --skip-type-check
  local base=$LH:61010 out=$LOGS/.c36
  curl -sf "$base/about" > "$out.about"
  grep -q 'ws-room:ws' "$out.about"        # comms ON by default
  grep -q 'localSceneParcels' "$out.about"
  curl -sf -H 'accept: text/html' "$base/" | grep -q '<html'   # browser landing page
  code "$base/" | grep -q 307              # non-browser -> redirect /about
  curl -sf "$base/scenes" | grep -q '"scenes"'
  post_jsonf '{"pointers":[]}' "$base/content/entities/active" > "$out.active"
  grep -q '"content"' "$out.active"
  post_jsonf '{"pointers":["0,0"]}' "$base/content/entities/active" | grep -q '"pointers"'
  curl -sf "$base/content/entities/scene" > /dev/null
  local roothash file_hash
  roothash=$(python3 -c "
import json; a=json.load(open('$out.about'))
urn=a['configurations']['scenesUrn'][0]
print(urn.split('entity:')[1].split('?')[0])")
  curl -sf "$base/content/contents/$roothash" | grep -q '"type":"scene"'
  file_hash=$(python3 -c "
import json; e=json.load(open('$out.active'))
ent=e[0] if isinstance(e, list) else e
print([c['hash'] for c in ent['content'] if c['file'].endswith('index.js')][0])")
  local etag
  etag=$(curl -sfD - -o /dev/null "$base/content/contents/$file_hash" | sed -n 's/^etag: //Ip' | tr -d '\r')
  test -n "$etag"
  curl -sf -o /dev/null -w '%{http_code}' -H "If-None-Match: $etag" \
    "$base/content/contents/$file_hash" | grep -q 304
  curl -sf -I "$base/content/contents/$file_hash" > /dev/null   # HEAD
  local machine
  machine=$(python3 -c "
import base64
h='$roothash'[4:]
h += '=' * (-len(h) % 4)
try: raw = base64.b64decode(h).decode()
except Exception: raw = base64.urlsafe_b64decode(h).decode()
print(raw.rsplit('-',1)[1])")
  test -n "$machine"
  # ignored-under-root file -> 404 not-published; outside the root -> 403
  code "$base/content/contents/b64-$(printf '%s-%s' "$SCENE/package.json" "$machine" | base64 -w0)" | grep -q 404
  code "$base/content/contents/b64-$(printf '/etc/passwd-%s' "$machine" | base64 -w0)" | grep -q 403
  # a non-preview CID (no b64- prefix) falls to the upstream proxy (dead here)
  curl -s -o /dev/null "$base/content/contents/bafkreialabala"
  curl -sf "$base/mobile-preview" | grep -q '"ok"'
  curl -sf "$base/lambdas/explore/realms" | grep -q 'stub\|realm'
  curl -sf "$base/lambdas/contracts/servers" > /dev/null
  curl -s -o /dev/null "$base/lambdas/status"               # generic proxy path (dead upstream)
  curl -s -o /dev/null "$base/explorer/index.js"            # explorer proxy path (dead upstream)
  code "$base/inspector" | grep -q '30[0-9]\|404'
  code "$base/inspector/" | grep -q 404
  code "$base/inspector/../../../etc/passwd" | grep -q 404
  code "$base/definitely-not-a-route" | grep -q 404
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61010/" 3 "$out.ws"
  grep -q 'OPEN' "$out.ws"          # reload socket upgrades; SCENE_UPDATE frame = c41/c102
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61010/mini-comms/room-1" 2 "$out.comms" rfc5
  grep -q 'OPEN' "$out.comms"
  stop_server
}
c37_start_offline_comms() {
  STARTDIR=$SCENE start_static 61011 --skip-build --offline-comms
  curl -sf "$LH:61011/about" | grep -q 'offline:offline'
  stop_server
}
c38_start_data_layer() {
  STARTDIR=$SCENE start_static 61012 --skip-build --data-layer
  local out=$LOGS/.c38
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61012/data-layer" 2 "$out.dl"
  grep -q 'OPEN' "$out.dl"
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61012/data-layer" 2 "$out.dl-evil" '' 'http://evil.example'
  grep -q 'HTTP 403\|ERROR' "$out.dl-evil"
  curl -s "$LH:61012/inspector/" | grep -q '@dcl/inspector'
  stop_server
}
c39_start_mobile() {
  STARTDIR=$SCENE start_static 61013 --skip-build --mobile
  curl -sf "$LH:61013/mobile-preview" > "$LOGS/.c39" || true
  grep -qi 'qr\|mobile\|LAN' "$LOGS/.start-61013.log"
  stop_server
}
c40_start_workspace() {
  STARTDIR=$WSROOT start_static 61014 --skip-type-check
  local about=$LOGS/.c40-about
  curl -sf "$LH:61014/about" > "$about"
  grep -q '60,60' "$about" && grep -q '61,60' "$about"
  python3 -c "
import json; a=json.load(open('$about'))
assert len(a['configurations']['scenesUrn']) == 2, a['configurations']['scenesUrn']"
  post_jsonf '{"pointers":["61,60"]}' "$LH:61014/content/entities/active" | grep -q '61,60'
  stop_server
}
c41_start_watch_reload() {
  local d=$WORK/watch-live; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  write_composite "$d/assets/scene/main.composite"
  STARTDIR=$d start_and_wait 61015 --ci --no-asset-bundles --skip-type-check
  local out=$LOGS/.c41
  spawn "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61015/" 25 "$out.ws"
  sleep 1
  echo 'export function main() { console.log(3) }' > "$d/src/index.ts"   # ts rebuild
  sleep 6
  mkdir -p "$d/models"
  printf 'glTF-stub' > "$d/models/prop.glb"          # UMT_CHANGE
  sleep 3
  rm "$d/models/prop.glb"                            # UMT_REMOVE
  sleep 3
  write_composite "$d/assets/scene/main.composite"   # composite regen
  sleep 8
  grep -q 'TEXT.*SCENE_UPDATE' "$out.ws"
  grep -c 'BIN ' "$out.ws" | grep -qv '^0$'
  stop_server
}
c42_start_skip_build() {
  STARTDIR=$SCENE start_static 61016 --skip-build
  curl -sf "$LH:61016/about" > /dev/null
  stop_server
}
c43_start_flag_compat_deeplink() {
  STARTDIR=$SCENE start_and_wait 61017 --ci --no-watch --skip-build --asset-bundles \
    --mcp --mcp-port 61098 --no-browser --skip-install --multi-instance \
    --no-client --ignoreComposite --mobile -- --foo=bar --lone
  local log=$LOGS/.start-61017.log
  grep -q 'local-ab=true' "$log"
  grep -q 'mcp=true' "$log"
  grep -q 'foo=bar' "$log"
  grep -q 'lone=true' "$log"
  grep -q -- '--no-browser has no effect' "$log"
  stop_server
}
c44_start_abgen_hint() {
  STARTDIR=$SCENE ABGEN_BIN=/nonexistent/abgen start_and_wait 61018 --ci --no-watch --skip-build
  grep -qi 'abgen\|asset' "$LOGS/.start-61018.log"
  stop_server
}
c45_start_default_port() {
  ( cd "$SCENE" && exec "$BIN" start --ci --no-watch --no-asset-bundles --skip-build ) \
    > "$LOGS/.start-auto.log" 2>&1 &
  local pid=$!; PIDS+=("$pid")
  local i=0 port=
  until port=$(sed -n 's/.*http:\/\/127\.0\.0\.1:\([0-9]*\)\/about.*/\1/p;s/.*realm=http:\/\/127\.0\.0\.1:\([0-9]*\).*/\1/p' "$LOGS/.start-auto.log" | grep -m1 .); do
    i=$((i + 1)); [ $i -ge 60 ] && break; sleep 1
  done
  if [ -z "$port" ]; then
    port=$(grep -oE '127\.0\.0\.1:[0-9]+' "$LOGS/.start-auto.log" | grep -m1 -oE '[0-9]+$')
  fi
  test -n "$port"
  wait_http "$LH:$port/about" 30
  kill -9 "$pid"
}
c46_tunnel_help() { "$BIN" start --tunnel help | grep -qi 'ssh'; }
c47_tunnel_invalid_url() {
  fails "$BIN" start --dir "$SCENE" --tunnel 'ftp://x' --ci --no-watch --skip-build 2> "$LOGS/.c47"
  grep -q 'invalid --tunnel URL' "$LOGS/.c47" && grep -q -- '--tunnel help' "$LOGS/.c47"
}
c48_tunnel_unreachable_warns() {
  STARTDIR=$SCENE start_static 61019 --skip-build --tunnel wss://127.0.0.1:1
  wait_for 20 grep -qi 'tunnel' "$LOGS/.start-61019.log"
  curl -sf "$LH:61019/about" > /dev/null
  stop_server
}
c49_start_port_in_use() {
  spawn python3 -c 'import socket, time
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("0.0.0.0", 61021)); s.listen(1)
print("BOUND", flush=True); time.sleep(120)'
  local blocker=$LAST_PID
  wait_for 15 tcp_ok 61021
  fails timeout 60 "$BIN" start --dir "$SCENE" -p 61021 --ci --no-watch --skip-build --no-asset-bundles 2> "$LOGS/.c49"
  grep -qi 'in use\|port' "$LOGS/.c49"
  kill -9 "$blocker"
}

c51_deploy_dry_run_deterministic() {
  "$BIN" deploy --dir "$SCENE" --dry-run --skip-build --timestamp 1700000000000 \
    --entity-out "$WORK/entity-1.json" > "$LOGS/.c51a"
  "$BIN" deploy --dir "$SCENE" --dry-run --skip-build --timestamp 1700000000000 \
    --entity-out "$WORK/entity-2.json" > "$LOGS/.c51b"
  grep -q 'entityId=' "$LOGS/.c51a"
  cmp "$WORK/entity-1.json" "$WORK/entity-2.json"
  diff <(grep entityId "$LOGS/.c51a") <(grep entityId "$LOGS/.c51b")
}
c52_deploy_key_env_to_content() {
  kdeploy "$DEPLOY_MIN" --target-content "$STUB" > "$LOGS/.c52.out" 2> "$LOGS/.c52.err"
  grep -q 'DCL_PRIVATE_KEY from the environment' "$LOGS/.c52.err"
  grep -q 'not Genesis City' "$LOGS/.c52.out" || grep -q 'not Genesis City' "$LOGS/.c52.err"
}
c53_deploy_sign_key_via_target() {
  "$BIN" deploy --dir "$DEPLOY_MIN" --skip-build --ci --sign-key "$WORK/sign.key" \
    --target "$STUB" > "$LOGS/.c53" 2>&1
  grep -q 'POST /content/entities' "$SREQ"
}
c54_deploy_key_needs_target() {
  fails kdeploy "$DEPLOY_MIN" 2> "$LOGS/.c54"
  grep -qi 'target' "$LOGS/.c54"
}
c55_deploy_world_needs_server() {
  fails "$BIN" deploy --dir "$WORLD_MIN" --skip-build --ci 2> "$LOGS/.c55"
  grep -qi 'world\|server' "$LOGS/.c55"
}
c56_deploy_conflicting_targets() {
  fails "$BIN" deploy --dir "$DEPLOY_MIN" --skip-build --ci --target "$STUB" \
    --target-content "$STUB" 2> "$LOGS/.c56"
  grep -qi 'both\|conflict\|together' "$LOGS/.c56"
}
c57_deploy_linker_flow_timeout() {
  linker_bg 6 "$LOGS/.c57" "$BIN" deploy --dir "$DEPLOY_MIN" --skip-build \
    --no-browser --port 61044 --target-content "$STUB"
  wait_http "$LH:61044/api/info" 15
  curl -sf "$LH:61044/" | grep -qi 'sign\|deploy\|wallet'
  curl -sf "$LH:61044/api/info" | grep -q 'entityId\|payload'
  post_json '{"signature":"0xdeadbeef","address":"'$ADDR'"}' "$LH:61044/api/sign" > /dev/null
  wait "$LINKER_PID" && return 1
  grep -qi 'timed out\|timeout' "$LOGS/.c57"
}
c58_deploy_world_key_yes_fallback() {
  kdeploy "$WORLD_MIN" --yes --target-content "$STUB" > "$LOGS/.c58" 2>&1
  grep -q 'DELETE /entities/example.dcl.eth' "$SREQ"
  grep -q 'DELETE /world/example.dcl.eth/scenes/5,5' "$SREQ"
}
c59_deploy_world_prompt_non_tty() {
  fails kdeploy "$WORLD_MIN" --target-content "$STUB" > "$LOGS/.c59" 2>&1
  grep -qi 'yes\|confirm\|continue' "$LOGS/.c59"
}
c60_deploy_world_denied() {
  local d=$WORK/world-denied; rm -rf "$d"
  write_min_scene "$d" "0,0" ',"worldConfiguration":{"name":"denied.dcl.eth"}'
  fails kdeploy "$d" --yes --target-content "$STUB" > "$LOGS/.c60" 2>&1
  grep -qi 'permission\|denied\|not allowed' "$LOGS/.c60"
}
c61_deploy_default_target_catalyst() {
  DCL_ONE_SDK_DEFAULT_TARGET=$STUB kdeploy "$DEPLOY_MIN" > "$LOGS/.c61" 2>&1
  grep -q 'DCL_ONE_SDK_DEFAULT_TARGET catalyst' "$LOGS/.c61"
}
c62_deploy_default_target_content() {
  DCL_ONE_SDK_DEFAULT_TARGET=$STUB/nested kdeploy "$DEPLOY_MIN" > "$LOGS/.c62" 2>&1
  grep -q 'as a content server' "$LOGS/.c62"
}
c63_deploy_rotation_browser_flow() {
  DCL_ONE_SDK_CATALYST_ROTATION=$STUB DCL_ONE_SDK_LINKER_TIMEOUT_SECS=5 "$BIN" deploy \
    --dir "$DEPLOY_MIN" --skip-build --no-browser --yes > "$LOGS/.c63" 2>&1 || true
  grep -q 'DCL_ONE_SDK_CATALYST_ROTATION' "$LOGS/.c63"
}
c64_deploy_unreachable_server() {
  fails kdeploy "$DEPLOY_MIN" --target-content "$DEAD" 2> "$LOGS/.c64"
  grep -qi 'unreachable\|failed\|refused' "$LOGS/.c64"
}
c65_deploy_at_workspace_root() {
  fails "$BIN" deploy --dir "$WSROOT" --skip-build --ci 2> "$LOGS/.c65"
  grep -qi 'workspace' "$LOGS/.c65"
}

c66_unpublish_ok() {
  kunpub --parcel 52,-52 --target-content "$STUB/content" | grep -q 'Unpublished 52,-52'
}
c67_unpublish_404_hint() {
  fails kunpub --parcel 9,9 --target-content "$STUB" 2> "$LOGS/.c67"
  grep -q 'synced Genesis City' "$LOGS/.c67"
}
c68_unpublish_bad_parcel() {
  fails kunpub --parcel not-a-parcel --target-content "$STUB" 2> "$LOGS/.c68"
}
c69_unpublish_no_target() {
  fails kunpub --parcel 0,0 2> "$LOGS/.c69"
  grep -q 'DCL_ONE_SDK_DEFAULT_TARGET\|target' "$LOGS/.c69"
}
c70_unpublish_unreachable() {
  fails kunpub --parcel 0,0 --target-content "$DEAD" 2> "$LOGS/.c70"
}
c71_unpublish_no_key() {
  fails "$BIN" unpublish --parcel 0,0 --target-content "$STUB" 2> "$LOGS/.c71"
  grep -q 'DCL_PRIVATE_KEY' "$LOGS/.c71"
}

c72_world_settings_get() {
  "$BIN" world settings get stub.dcl.eth --target-content "$STUB" | grep -q 'Stub World'
}
c73_world_settings_get_unreachable() {
  fails "$BIN" world settings get stub.dcl.eth --target-content "$DEAD" 2> "$LOGS/.c73"
}
c74_world_settings_get_refused() {
  fails "$BIN" world settings get refused.dcl.eth --target-content "$STUB" 2> "$LOGS/.c74"
  grep -qi 'refused\|403\|permission' "$LOGS/.c74"
}
c75_world_settings_set_title() {
  wset stub.dcl.eth --title 'New Title' > "$LOGS/.c75" 2>&1
  grep -q 'PUT /world/stub.dcl.eth/settings' "$SREQ"
}
c76_world_settings_set_all_fields() {
  wset stub.dcl.eth --title T --description D --content-rating T \
    --spawn-coordinates 1,1 --skybox-time 12:00 --single-player true \
    --show-in-places false --category art --category games \
    --thumbnail "$WORK/thumb.png" > "$LOGS/.c76" 2>&1
  grep -q 'PUT /world/stub.dcl.eth/settings' "$SREQ"
}
c77_world_settings_set_nothing() {
  fails wset stub.dcl.eth 2> "$LOGS/.c77"
  grep -qi 'nothing to update' "$LOGS/.c77"
}
c78_world_settings_set_denied() {
  fails wset denied.dcl.eth --title X 2> "$LOGS/.c78"
  grep -qi 'refused\|denied\|403' "$LOGS/.c78"
}
c79_world_permissions_list() {
  "$BIN" world permissions list stub.dcl.eth --target-content "$STUB" | grep -qi 'deployment'
}
c80_world_permissions_grant() {
  wperm grant stub.dcl.eth deployment "$ADDR" > "$LOGS/.c80" 2>&1
  grep -q "PUT /world/stub.dcl.eth/permissions/deployment/$ADDR" "$SREQ"
}
c81_world_permissions_revoke() {
  wperm revoke stub.dcl.eth streaming "$ADDR" > "$LOGS/.c81" 2>&1
  grep -q "DELETE /world/stub.dcl.eth/permissions/streaming/$ADDR" "$SREQ"
}
c82_world_permissions_bad_perm() {
  fails wperm grant stub.dcl.eth frobnicate "$ADDR" 2> "$LOGS/.c82"
  grep -qi 'permission' "$LOGS/.c82"
}
c83_world_permissions_bad_address() {
  fails wperm grant stub.dcl.eth deployment not-an-address 2> "$LOGS/.c83"
  grep -qi 'address' "$LOGS/.c83"
}
c84_world_linker_flow_timeout() {
  linker_bg 6 "$LOGS/.c84" "$BIN" world settings set stub.dcl.eth \
    --target-content "$STUB" --title X --no-browser --port 61045
  wait_http "$LH:61045/api/info" 15
  curl -sf "$LH:61045/" > /dev/null
  curl -sf "$LH:61045/api/info" | grep -q 'payload\|method'
  post_json '{"signature":"0xdeadbeef","address":"'$ADDR'"}' "$LH:61045/api/sign" > /dev/null
  wait "$LINKER_PID" && return 1
  grep -qi 'timed out\|timeout' "$LOGS/.c84"
}
c85_world_set_unreachable() {
  fails "$BIN" world settings set stub.dcl.eth --target-content "$DEAD" \
    --sign-key "$WORK/sign.key" --title X 2> "$LOGS/.c85"
}

c86_pack_valid_wearable() {
  printf 'glTF-stub-model' > "$WEARABLE/model.glb"
  cp "$WORK/thumb.png" "$WEARABLE/thumbnail.png"
  "$BIN" pack --dir "$WEARABLE" | grep -q 'Smart wearable packed'
  test -s "$WEARABLE/smart-wearable.zip"
}
c87_pack_not_a_wearable() {
  fails "$BIN" pack --dir "$SCENE" 2> "$LOGS/.c87"
  grep -q 'not a smart wearable' "$LOGS/.c87"
}
c88_pack_schema_violation() {
  local d=$WORK/pack-bad-rarity; rm -rf "$d"; cp -a "$WEARABLE" "$d"; rm -f "$d/smart-wearable.zip"
  python3 - "$d/wearable.json" <<'EOF'
import json, sys
p = sys.argv[1]; w = json.load(open(p)); w["rarity"] = "ultra-shiny"
json.dump(w, open(p, "w"))
EOF
  fails "$BIN" pack --dir "$d" --skip-build 2> "$LOGS/.c88"
  grep -q 'rarity' "$LOGS/.c88"
}
c89_pack_malformed_wearable_json() {
  local d=$WORK/pack-malformed; rm -rf "$d"; cp -a "$WEARABLE" "$d"
  echo '{ nope' > "$d/wearable.json"
  fails "$BIN" pack --dir "$d" --skip-build 2> "$LOGS/.c89"
  grep -qi 'wearable.json' "$LOGS/.c89"
}
c90_pack_oversize_warns() {
  local d=$WORK/pack-oversize; rm -rf "$d"; cp -a "$WEARABLE" "$d"; rm -f "$d/smart-wearable.zip"
  python3 -c "open('$d/big.bin','wb').write(b'x' * (3 * 1024 * 1024))"
  "$BIN" pack --dir "$d" --skip-build > "$LOGS/.c90" 2>&1
  grep -qi 'MiB\|size\|exceeds\|warning' "$LOGS/.c90"
}
c91_pack_missing_referenced_file() {
  local d=$WORK/pack-missing-file; rm -rf "$d"; cp -a "$WEARABLE" "$d"
  rm "$d/model.glb"
  fails "$BIN" pack --dir "$d" --skip-build 2> "$LOGS/.c91"
  grep -q 'model.glb' "$LOGS/.c91"
}

c92_verbose_error_chain() {
  fails "$BIN" --verbose deploy --dir "$DEPLOY_MIN" --skip-build --ci \
    --target-content "$DEAD" --sign-key "$WORK/sign.key" 2> "$LOGS/.c92"
  grep -qi 'caused by' "$LOGS/.c92"
}
c93_rust_log_implies_verbose() {
  fails env RUST_LOG=info "$BIN" build --dir "$WORK/definitely-missing-dir" 2> "$LOGS/.c93"
  grep -q 'does not exist' "$LOGS/.c93"
}

c94_tunnel_real_service() {
  test -x "$TUNNEL_BIN" || skip "no catalyrst-preview-tunnel binary at $TUNNEL_BIN"
  echo tok1 > "$WORK/tunnel.token"
  spawn env HTTP_SERVER_HOST=127.0.0.1 HTTP_SERVER_PORT=61050 \
    PUBLIC_BASE_URL=$LH:61050 TUNNEL_TOKENS=tok1 "$TUNNEL_BIN"
  local tpid=$LAST_PID
  wait_for 20 tcp_ok 61050
  STARTDIR=$SCENE start_static 61051 --skip-build \
    --tunnel "$WS:61050" --tunnel-token-file "$WORK/tunnel.token"
  local log=$LOGS/.start-61051.log pub=
  wait_for 30 grep -qE 'http://127\.0\.0\.1:61050/t/[A-Za-z0-9_-]+' "$log"
  pub=$(grep -oE 'http://127\.0\.0\.1:61050/t/[A-Za-z0-9_-]+' "$log" | grep -m1 .)
  curl -sf "$pub/about" | grep -q 'scenesUrn'
  curl -sf "$pub/content/entities/scene" > /dev/null
  curl -s -o /dev/null -X PATCH "$pub/about"                              # unsupported method
  post_json '{"pointers":[]}' "$pub/content/entities/active" -o /dev/null # body forwarding
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "${pub/http:/ws:}/" 3 "$LOGS/.c94.ws"
  grep -q 'OPEN' "$LOGS/.c94.ws"    # reload WS multiplexed over the trunk upgrades
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "${pub/http:/ws:}/definitely-not-a-ws-route" 2 "$LOGS/.c94.badws" || true
  stop_server
  STARTDIR=$SCENE DCL_ONE_SDK_TUNNEL_TOKEN=wrong-token start_static 61057 --skip-build \
    --tunnel "$WS:61050"
  wait_for 15 grep -qi 'tunnel' "$LOGS/.start-61057.log"
  stop_server
  kill -9 "$tpid"
}
c95_deploy_linker_completed() {
  need_signer
  linker_bg 90 "$LOGS/.c95" "$BIN" deploy --dir "$DEPLOY_MIN" --skip-build \
    --no-browser --port 61052 --target-content "$STUB"
  wait_http "$LH:61052/api/info" 20
  local entity_id sig
  entity_id=$(curl -sf "$LH:61052/api/info" | jget entityId)
  sig=$(sign_message "$entity_id")
  post_jsonf '{"address":"'$ADDR'","signature":"'$sig'","entityId":"'$entity_id'"}' \
    "$LH:61052/api/sign" > "$LOGS/.c95.sign"
  wait "$LINKER_PID"
  grep -q 'Deployed' "$LOGS/.c95"
}
c96_world_linker_completed() {
  need_signer
  linker_bg 90 "$LOGS/.c96" "$BIN" world settings set stub2.dcl.eth \
    --target-content "$STUB" --title Z --no-browser --port 61053
  wait_http "$LH:61053/api/info" 20
  local payload sig
  payload=$(curl -sf "$LH:61053/api/info" | jget payload)
  sig=$(sign_message "$payload")
  printf '{"address":"%s","signature":"%s","payload":"%s"}' "$ADDR" "$sig" "$payload" > "$WORK/.c96.body"
  post_jsonf @"$WORK/.c96.body" "$LH:61053/api/sign" > "$LOGS/.c96.sign"
  wait "$LINKER_PID"
  grep -q 'PUT /world/stub2.dcl.eth/settings' "$SREQ"
}
c97_world_linker_denied_alive() {
  need_signer
  linker_bg 12 "$LOGS/.c97" "$BIN" world settings set denied.dcl.eth \
    --target-content "$STUB" --title Z --no-browser --port 61058
  wait_http "$LH:61058/api/info" 20
  local payload sig
  payload=$(curl -sf "$LH:61058/api/info" | jget payload)
  sig=$(sign_message "$payload")
  printf '{"address":"%s","signature":"%s","payload":"%s"}' "$ADDR" "$sig" "$payload" > "$WORK/.c97.body"
  post_json @"$WORK/.c97.body" "$LH:61058/api/sign" > "$LOGS/.c97.sign"
  kill -0 "$LINKER_PID" 2>/dev/null   # 403 keeps the page alive
  fails wait "$LINKER_PID"            # then the timeout ends it non-zero
}
c98_vendor_chunks() {
  need_npm_scene "no full npm scene for vendor-chunks"
  local d=$WORK/vendor-chunks; npm_scene_clone "$d"
  "$BIN" vendor-chunks --dir "$d" --out-core "$WORK/chunk-core.js" \
    --out-smart "$WORK/chunk-smart.js" > "$LOGS/.c98" 2>&1
  test -s "$WORK/chunk-core.js" && test -s "$WORK/chunk-smart.js"
}
c99_build_crdt_via_sdk_commands() {
  need_npm_scene
  local d=$WORK/crdt-sdk-commands; npm_scene_clone "$d"
  write_composite "$d/assets/scene/main.composite"
  "$BIN" build --dir "$d" --skip-type-check > "$LOGS/.c99" 2>&1
  test -f "$d/main.crdt"
}
c100_start_backfill_proxy() {
  mkdir -p "$WORK/stub-contents"
  local cid
  cid=$(grep ' scene.json ' "$LOGS/.c51a" | cut -d' ' -f1)
  test -n "$cid"
  cp "$DEPLOY_MIN/scene.json" "$WORK/stub-contents/$cid"
  STARTDIR=$SCENE DCL_ONE_SDK_CATALYST=$STUB DCL_ONE_SDK_CATALYST_ROTATION=$DEAD,$STUB \
    start_static 61054 --skip-build
  local base=$LH:61054
  curl -sf "$base/content/contents/$cid" | grep -q '52,-52'      # back-fill via stub
  curl -sf "$base/content/contents/$cid" | grep -q '52,-52'      # cache hit
  test -f "$SCENE/.dcl-cache/contents/$cid"
  curl -s -o /dev/null "$base/lambdas/collections/wearables"     # proxy w/ rotation fallback
  post_json '{}' "$base/content/entities" > /dev/null            # deploy proxy
  post_json '{"pointers":["urn:decentraland:matic:collections-v2:0xabc:1"]}' \
    "$base/content/entities/active" > /dev/null                  # upstream pointer resolve
  stop_server
}
c101_start_inspector_ui_injected() {
  local ui=$WORK/fake-inspector; rm -rf "$ui"; mkdir -p "$ui/public/assets"
  cat > "$ui/public/index.html" <<'EOF'
<!doctype html><script>
const config = '$CONFIG'
if (config !== '$CONFIG') { window.cfg = JSON.parse(config) }
</script>
EOF
  echo 'console.log("bundle")' > "$ui/public/assets/bundle.js"
  STARTDIR=$SCENE DCL_ONE_INSPECTOR_DIR=$ui start_static 61055 --skip-build --data-layer
  local base=$LH:61055
  curl -sf "$base/inspector/" > "$LOGS/.c101.html"
  grep -q 'dataLayerRpcWsUrl' "$LOGS/.c101.html"                 # $CONFIG injected
  grep -q "!== '\$CONFIG'" "$LOGS/.c101.html"                    # sentinel untouched
  curl -sf "$base/inspector/assets/bundle.js" | grep -q bundle
  code "$base/inspector/nope.js" | grep -q 404
  stop_server
  local bad=$WORK/fake-inspector-bad; mkdir -p "$bad"
  fails env DCL_ONE_INSPECTOR_DIR=$bad timeout 60 "$BIN" start --dir "$SCENE" -p 61059 --ci \
      --no-watch --no-asset-bundles --skip-build --data-layer 2> "$LOGS/.c101.bad"
  grep -q 'DCL_ONE_INSPECTOR_DIR' "$LOGS/.c101.bad"
}
c102_start_failed_build_recovers() {
  local d=$WORK/start-broken; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  echo 'export function main() { const x = = 1 }' > "$d/src/index.ts"
  STARTDIR=$d start_and_wait 61056 --ci --no-asset-bundles --skip-type-check
  grep -qi 'build failed\|error' "$LOGS/.start-61056.log"
  spawn "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61056/" 15 "$LOGS/.c102.ws"
  sleep 1
  echo 'export function main() { console.log(9) }' > "$d/src/index.ts"
  wait_for 30 grep -q 'TEXT.*SCENE_UPDATE' "$LOGS/.c102.ws"
  stop_server
}
c103_start_config_error_fatal() {
  local d=$WORK/start-config-error; rm -rf "$d"; mkdir -p "$d"
  printf '{"runtimeVersion":"7","scene":{"parcels":["0,0"],"base":"0,0"}}' > "$d/scene.json"
  fails timeout 60 "$BIN" start --dir "$d" -p 61060 --ci --no-watch --no-asset-bundles 2> "$LOGS/.c103"
  grep -qi 'main' "$LOGS/.c103"
}
c104_deploy_multi_scene_world() {
  kdeploy "$WORLD_MIN" --multi-scene --target-content "$STUB" > "$LOGS/.c104" 2>&1
  grep -q 'Deployed\|HTTP 200' "$LOGS/.c104"
}
c105_pack_repack_replaces_zip() {
  test -f "$WEARABLE/smart-wearable.zip"
  "$BIN" pack --dir "$WEARABLE" --skip-build | grep -q 'Smart wearable packed'
}

c106_deploy_consent_refused_ci() {
  # The consent gate fires only when the target comes off the real public
  # rotation; an env rotation is treated as an explicit choice. Not reachable
  # offline — kept in the inventory as needs-external-world.
  skip "public-network consent gate walks the real catalyst rotation"
}
c107_deploy_world_prompt_pty() {
  need_pty
  printf 'n\n' | script -qec "env DCL_PRIVATE_KEY=$KEY $BIN deploy --dir $WORLD_MIN --skip-build --target-content $STUB" /dev/null > "$LOGS/.c107n" || true
  grep -qi 'cancelled' "$LOGS/.c107n"
  printf 'y\n' | script -qec "env DCL_PRIVATE_KEY=$KEY $BIN deploy --dir $WORLD_MIN --skip-build --target-content $STUB" /dev/null > "$LOGS/.c107y" || true
  grep -q 'Deployed\|HTTP 200' "$LOGS/.c107y"
}
c108_deploy_world_parcel_allowlist() {
  local d=$WORK/world-parcels; rm -rf "$d"
  write_min_scene "$d" "0,0" ',"worldConfiguration":{"name":"parcels.dcl.eth"}'
  kdeploy "$d" --yes --target-content "$STUB" > "$LOGS/.c108a" 2>&1
  grep -q 'Deployed\|HTTP 200' "$LOGS/.c108a"
  local dd=$WORK/world-parcels-denied; rm -rf "$dd"
  write_min_scene "$dd" "0,0" ',"worldConfiguration":{"name":"parcels-denied.dcl.eth"}'
  fails kdeploy "$dd" --yes --target-content "$STUB" 2> "$LOGS/.c108b"
  grep -q 'no permission to deploy' "$LOGS/.c108b" && grep -q '0,0' "$LOGS/.c108b"
}
c109_deploy_server_rejects() {
  fails kdeploy "$DEPLOY_MIN" --target-content "$STUB/reject" 2> "$LOGS/.c109"
  grep -q 'rejected this deployment (HTTP 500)' "$LOGS/.c109"
}
c110_world_deploy_linker_del_sig() {
  need_signer
  local d=$WORK/linker-world; rm -rf "$d"
  write_min_scene "$d" "0,0" ',"worldConfiguration":{"name":"linkerworld.dcl.eth"}'
  linker_bg 90 "$LOGS/.c110" "$BIN" deploy --dir "$d" --skip-build \
    --yes --no-browser --port 61061 --target-content "$STUB"
  wait_http "$LH:61061/api/info" 20
  local info entity_id delete_payload sig dsig
  info=$(curl -sf "$LH:61061/api/info")
  entity_id=$(echo "$info" | jget entityId)
  delete_payload=$(echo "$info" | jget deletePayload)
  sig=$(sign_message "$entity_id")
  post_json '{"address":"'$ADDR'","signature":"'$sig'","entityId":"'$entity_id'"}' \
    "$LH:61061/api/sign" | grep -q 'second signature'
  info=$(curl -sf "$LH:61061/api/info")   # the failed attempt consumed the id
  entity_id=$(echo "$info" | jget entityId)
  delete_payload=$(echo "$info" | jget deletePayload)
  sig=$(sign_message "$entity_id")
  dsig=$(sign_message "$delete_payload")
  post_json '{"address":"'$ADDR'","signature":"'$sig'","entityId":"'$entity_id'","deleteSignature":"'$dsig'"}' \
    "$LH:61061/api/sign" > "$LOGS/.c110.sign"
  wait "$LINKER_PID"
  grep -q 'Deployed' "$LOGS/.c110"
}
c111_deploy_linker_no_xdg_open() {
  mkdir -p "$WORK/emptybin"
  fails env PATH=$WORK/emptybin DCL_ONE_SDK_LINKER_TIMEOUT_SECS=3 "$BIN" deploy \
    --dir "$DEPLOY_MIN" --skip-build --port 61062 --target-content "$STUB" > "$LOGS/.c111" 2>&1
  grep -q 'could not open a browser automatically' "$LOGS/.c111"
}
c112_start_workspace_watch() {
  STARTDIR=$WSROOT start_and_wait 61063 --ci --no-asset-bundles --skip-type-check
  curl -sf "$LH:61063/about" | grep -q '60,60'
  stop_server
}
c113_start_data_layer_no_node() {
  mkdir -p "$WORK/emptybin"
  fails timeout 60 env PATH=$WORK/emptybin "$BIN" start --dir "$SCENE" -p 61064 --ci \
    --no-watch --no-asset-bundles --skip-build --data-layer 2> "$LOGS/.c113"
  grep -q 'node is required' "$LOGS/.c113"
}
c114_start_data_layer_broken() {
  local d=$WORK/dl-broken; rm -rf "$d"; mkdir -p "$d"
  cp "$SCENE/scene.json" "$SCENE/package.json" "$SCENE/tsconfig.json" "$d/"
  cp -a "$SCENE/bin" "$d/bin" 2>/dev/null || true
  mkdir -p "$d/src"; cp "$SCENE/src/index.ts" "$d/src/"
  cp -al "$SCENE/node_modules" "$d/node_modules"
  rm -rf "$d/node_modules/ws" "$d/node_modules/@dcl/rpc" "$d/node_modules/@dcl/inspector"
  fails timeout 90 "$BIN" start --dir "$d" -p 61065 --ci --no-watch --no-asset-bundles \
    --skip-build --data-layer 2> "$LOGS/.c114"
  grep -q 'data-layer host did not come up' "$LOGS/.c114"
}
c115_build_opera_composite() {
  need_npm_scene
  local crate_dir; crate_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
  test -f "$crate_dir/testdata/opera-main.composite" || skip "no opera testdata"
  local d=$WORK/opera-scene; npm_scene_clone "$d"
  mkdir -p "$d/assets/scene"
  cp "$crate_dir/testdata/opera-main.composite" "$d/assets/scene/main.composite"
  "$BIN" build --dir "$d" --skip-type-check > "$LOGS/.c115" 2>&1
  test -f "$d/main.crdt"
}
c116_start_abgen_fake_sidecar() {
  tcp_ok 5147 || skip "5147 free — a fake sidecar would claim abgen's canonical port"
  cat > "$WORK/fake-abgen" <<'EOF'
#!/usr/bin/env bash
echo 'ABGEN_BUILD {"file":"models/prop.glb","platform":"webgl","build_ms":12,"out_bytes":42,"result":"ok"}'
echo 'ABGEN_BUILD {"file":"models/prop.glb","platform":"webgl","result":"shader stage failed"}'
echo 'WARN something odd but survivable'
echo 'plain chatter line'
exec python3 -c '
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
class H(BaseHTTPRequestHandler):
    def log_message(self, f, *a): pass
    def do_GET(self):
        self.send_response(200); self.send_header("Content-Length", "2")
        self.end_headers(); self.wfile.write(b"ok")
ThreadingHTTPServer(("127.0.0.1", int(os.environ["HTTP_SERVER_PORT"])), H).serve_forever()
'
EOF
  chmod +x "$WORK/fake-abgen"
  STARTDIR=$SCENE ABGEN_BIN=$WORK/fake-abgen start_and_wait 61066 --ci --no-watch --skip-build
  local log=$LOGS/.start-61066.log i=0
  until grep -q 'abgen build: models/prop.glb' "$log" && grep -q 'optimized-assets-url' "$log"; do
    i=$((i + 1)); [ $i -ge 30 ] && { stop_server; return 1; }; sleep 1
  done
  grep -q 'abgen build FAIL' "$log"
  stop_server
  printf '#!/usr/bin/env bash\necho boom >&2\nexit 3\n' > "$WORK/fake-abgen-crash"
  chmod +x "$WORK/fake-abgen-crash"
  STARTDIR=$SCENE ABGEN_BIN=$WORK/fake-abgen-crash start_and_wait 61067 --ci --no-watch --skip-build
  wait_for 20 grep -qi 'abgen' "$LOGS/.start-61067.log" || { stop_server; return 1; }
  stop_server
}
c118_tunnel_token_file_missing() {
  fails "$BIN" start --dir "$SCENE" --skip-build --ci --no-watch \
    --tunnel wss://127.0.0.1:1 --tunnel-token-file "$WORK/no-such-token-file" 2> "$LOGS/.c118"
  grep -qi 'token' "$LOGS/.c118"
}
c119_start_watch_registry_flip() {
  need_npm_scene
  local d=$WORK/registry-flip; npm_scene_clone "$d"
  write_composite "$d/assets/scene/main.composite"
  STARTDIR=$d start_and_wait 61068 --ci --no-asset-bundles --skip-type-check
  add_script_component "$d/assets/scene/main.composite"
  wait_for 30 grep -q 'rebuilt' "$LOGS/.start-61068.log"
  curl -sf "$LH:61068/about" > /dev/null
  stop_server
}
c120_landing_rich_scene() {
  local d=$WORK/landing-rich; rm -rf "$d"; mkdir -p "$d"
  cp -a "$SCENE/bin" "$d/bin"; cp "$SCENE/package.json" "$SCENE/tsconfig.json" "$d/"
  mkdir -p "$d/src"; cp "$SCENE/src/index.ts" "$d/src/"
  cp -al "$SCENE/node_modules" "$d/node_modules"
  python3 - "$d/scene.json" "$SCENE/scene.json" <<'EOF'
import json, sys
out, src = sys.argv[1], sys.argv[2]
s = json.load(open(src))
s["spawnPoints"] = [
    {"name": "gate", "default": True, "position": {"x": 1, "y": 0, "z": 1},
     "cameraTarget": {"x": 8, "y": 1, "z": 8}},
    {"name": "roof", "position": {"x": [0, 3], "y": 0, "z": 2}}]
s["featureToggles"] = {"voiceChat": "disabled", "portableExperiences": "enabled"}
json.dump(s, open(out, "w"))
EOF
  STARTDIR=$d start_static 61069 --skip-build
  curl -sf -H 'accept: text/html' "$LH:61069/" > "$LOGS/.c120.html"
  grep -q 'looks at' "$LOGS/.c120.html"
  grep -q 'featureToggles.voiceChat' "$LOGS/.c120.html"
  stop_server
  STARTDIR=$WSROOT start_static 61070 --skip-type-check
  curl -sf -H 'accept: text/html' "$LH:61070/" | grep -q 'also in this realm'
  stop_server
}
c121_start_privileged_port() {
  [ "$(id -u)" -ne 0 ] || skip "running as root — privileged ports would bind"
  fails timeout 60 "$BIN" start --dir "$SCENE" -p 900 --ci --no-watch --skip-build \
    --no-asset-bundles 2> "$LOGS/.c121"
  grep -q 'elevated rights' "$LOGS/.c121"
}
c122_data_layer_allowed_origins() {
  STARTDIR=$SCENE DCL_ONE_SDK_ALLOWED_ORIGINS='http://evil.example, http://other.example' \
    start_static 61071 --skip-build --data-layer
  "$NODE" "$WORK/ws-probe.mjs" "$SCENE" "$WS:61071/data-layer" 2 "$LOGS/.c122.ws" '' 'http://evil.example'
  grep -q 'OPEN' "$LOGS/.c122.ws"
  stop_server
}
c123_build_no_node_for_typecheck() {
  mkdir -p "$WORK/emptybin"
  fails env PATH=$WORK/emptybin "$BIN" build --dir "$SCENE" 2> "$LOGS/.c123"
  grep -q 'node is required' "$LOGS/.c123"
}
c124_build_watch_typecheck_rebuilds() {
  local d=$WORK/watch-tc; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  spawn "$BIN" build --dir "$d" --watch
  local wpid=$LAST_PID log=$LOGS/124-build_watch_typecheck_rebuilds.log
  wait_for 120 grep -q 'Watching for changes' "$log" 2>/dev/null
  echo 'export function main() { console.log(4) }' > "$d/src/index.ts"
  wait_for 90 grep -q 'rebuilt' "$log" 2>/dev/null
  kill -INT "$wpid"; sleep 1; kill -9 "$wpid" 2>/dev/null || true
}
c125_build_smart_chunk_missing() {
  local d=$WORK/smart-chunk-missing; rm -rf "$d"
  "$BIN" init --dir "$d" --project scene
  write_composite "$d/assets/scene/main.composite"
  add_script_component "$d/assets/scene/main.composite"
  rm -f "$d/node_modules/@dcl/sdk/prebuilt/smart.js"
  fails "$BIN" build --dir "$d" --skip-type-check 2> "$LOGS/.c125"
  grep -qi 'smart' "$LOGS/.c125"
}
# Compare-only: build a golden fixture with THIS binary and check the runtime
# harness still prints what tests/golden.rs recorded. The tour and the Rust
# suite share the one golden file, so they cannot drift apart quietly; the tour
# never rewrites it, which is why UPDATE_GOLDEN is absent here.
c126_golden_runtime_compare() {
  test -n "$NODE" || skip "no node for the golden runtime harness"
  local d=$WORK/golden-cube
  rm -rf "$d"; cp -R "$CRATE/testdata/golden/cube" "$d"
  "$BIN" init --dir "$d" --node-modules-only > "$LOGS/.c126.init" 2>&1
  "$BIN" build --dir "$d" --production > "$LOGS/.c126.build" 2>&1
  "$NODE" "$CRATE/scripts/golden-runtime.mjs" "$d" > "$LOGS/.c126.actual" 2>&1
  sed -n '/^EVAL /,$p' "$CRATE/testdata/golden/cube.production.golden" > "$LOGS/.c126.want"
  diff -u "$LOGS/.c126.want" "$LOGS/.c126.actual"
}

echo "coverage tour: bin=$BIN work=$WORK filter=$FILTER"
setup_fixtures

# The table reads on fd 9 so case commands keep their own stdin.
while read -r n f <&9; do tcase "$n" "$f" "c${n}_${f}"; done 9<<'EOF'
01 version
02 help_top
03 help_subcommands
04 unknown_flag
05 unknown_subcommand
06 no_args
07 init_scene
08 init_refuses_non_empty
09 init_yes_non_empty
10 init_smart_wearable
11 init_node_modules_only
12 init_non_tty_defaults_scene
13 init_tty_prompt_wearable
14 init_tty_prompt_bad_answer
15 init_target_is_file
16 ctx_offline_in_project
17 ctx_outside_project
18 ctx_download_from_stub
19 ctx_unreachable_api
20 ctx_partial_failure
21 build_dev
22 build_production
23 build_skip_type_check
24 build_custom_entry_point
25 build_composite_scene
26 build_ignore_composite
27 build_workspace
28 build_syntax_error
29 build_type_error
30 build_broken_composite
31 build_oversize_composite
32 build_watch_edit_rebuilds
33 build_watch_workspace_rebuilds
34 build_missing_tsconfig
35 build_workspace_broken_member
36 start_probe_suite
37 start_offline_comms
38 start_data_layer
39 start_mobile
40 start_workspace
41 start_watch_reload
42 start_skip_build
43 start_flag_compat_deeplink
44 start_abgen_hint
45 start_default_port
46 tunnel_help
47 tunnel_invalid_url
48 tunnel_unreachable_warns
49 start_port_in_use
51 deploy_dry_run_deterministic
52 deploy_key_env_to_content
53 deploy_sign_key_via_target
54 deploy_key_needs_target
55 deploy_world_needs_server
56 deploy_conflicting_targets
57 deploy_linker_flow_timeout
58 deploy_world_key_yes_fallback
59 deploy_world_prompt_non_tty
60 deploy_world_denied
61 deploy_default_target_catalyst
62 deploy_default_target_content
63 deploy_rotation_browser_flow
64 deploy_unreachable_server
65 deploy_at_workspace_root
66 unpublish_ok
67 unpublish_404_hint
68 unpublish_bad_parcel
69 unpublish_no_target
70 unpublish_unreachable
71 unpublish_no_key
72 world_settings_get
73 world_settings_get_unreachable
74 world_settings_get_refused
75 world_settings_set_title
76 world_settings_set_all_fields
77 world_settings_set_nothing
78 world_settings_set_denied
79 world_permissions_list
80 world_permissions_grant
81 world_permissions_revoke
82 world_permissions_bad_perm
83 world_permissions_bad_address
84 world_linker_flow_timeout
85 world_set_unreachable
86 pack_valid_wearable
87 pack_not_a_wearable
88 pack_schema_violation
89 pack_malformed_wearable_json
90 pack_oversize_warns
91 pack_missing_referenced_file
92 verbose_error_chain
93 rust_log_implies_verbose
94 tunnel_real_service
95 deploy_linker_completed
96 world_linker_completed
97 world_linker_denied_alive
98 vendor_chunks
99 build_crdt_via_sdk_commands
100 start_backfill_proxy
101 start_inspector_ui_injected
102 start_failed_build_recovers
103 start_config_error_fatal
104 deploy_multi_scene_world
105 pack_repack_replaces_zip
106 deploy_consent_refused_ci
107 deploy_world_prompt_pty
108 deploy_world_parcel_allowlist
109 deploy_server_rejects
110 world_deploy_linker_del_sig
111 deploy_linker_no_xdg_open
112 start_workspace_watch
113 start_data_layer_no_node
114 start_data_layer_broken
115 build_opera_composite
116 start_abgen_fake_sidecar
118 tunnel_token_file_missing
119 start_watch_registry_flip
120 landing_rich_scene
121 start_privileged_port
122 data_layer_allowed_origins
123 build_no_node_for_typecheck
124 build_watch_typecheck_rebuilds
125 build_smart_chunk_missing
126 golden_runtime_compare
EOF

echo
echo "==== coverage tour summary: $PASS pass, $FAIL fail, $SKIP skip ===="
for c in "${FAILED_CASES[@]:-}"; do [ -n "$c" ] && echo "FAILED: $c"; done
exit "$FAIL"
