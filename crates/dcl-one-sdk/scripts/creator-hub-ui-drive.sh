#!/usr/bin/env bash
set -euo pipefail

SCENE=${1:?usage: creator-hub-ui-drive.sh <scene-dir> <evidence-dir> [preview-port] [cdp-port]}
EVIDENCE=${2:?usage: creator-hub-ui-drive.sh <scene-dir> <evidence-dir> [preview-port] [cdp-port]}
PORT=${3:-5734}
CDP=${4:-5735}

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BIN=${DCL_ONE_SDK_BIN:-$SCRIPT_DIR/../../../target/debug/dcl-one-sdk}
CHROMIUM=${CHROMIUM:-$(ls -d /nix/store/*chromium*/bin/chromium 2>/dev/null | grep -v ungoogled | head -1)}
[ -x "$BIN" ] || { echo "FAIL: dcl-one-sdk binary not found at $BIN (set DCL_ONE_SDK_BIN)"; exit 1; }
[ -n "$CHROMIUM" ] && [ -x "$CHROMIUM" ] || { echo "FAIL: chromium not found (set CHROMIUM)"; exit 1; }

mkdir -p "$EVIDENCE"
PROFILE=$(mktemp -d)
SERVER_PID=""
CHROME_PID=""
cleanup() {
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true
  [ -n "$CHROME_PID" ] && kill "$CHROME_PID" 2>/dev/null || true
  pkill -P $$ 2>/dev/null || true
  rm -rf "$PROFILE"
}
trap cleanup EXIT

"$BIN" start --dir "$SCENE" -p "$PORT" --data-layer --offline-comms \
  > "$EVIDENCE/start.log" 2>&1 &
SERVER_PID=$!

for i in $(seq 1 120); do
  curl -sf -o /dev/null "http://127.0.0.1:$PORT/about" && break
  kill -0 "$SERVER_PID" 2>/dev/null || { echo "FAIL: server exited early"; tail -20 "$EVIDENCE/start.log"; exit 1; }
  sleep 1
done
curl -sf -o /dev/null "http://127.0.0.1:$PORT/inspector/" || { echo "FAIL: /inspector/ not serving"; exit 1; }

"$CHROMIUM" --headless=new --no-sandbox --disable-gpu-sandbox \
  --remote-debugging-port="$CDP" --user-data-dir="$PROFILE" --window-size=1600,900 \
  about:blank > "$EVIDENCE/chromium.log" 2>&1 &
CHROME_PID=$!
for i in $(seq 1 30); do
  curl -sf -o /dev/null "http://127.0.0.1:$CDP/json/version" && break
  sleep 1
done

node "$SCRIPT_DIR/creator-hub-ui-drive.mjs" "$SCENE" "$PORT" "$CDP" "$EVIDENCE"
RC=$?
grep -E "main.crdt|SCENE_UPDATE|warning" "$EVIDENCE/start.log" | tail -20 > "$EVIDENCE/server-highlights.log" || true
exit $RC
