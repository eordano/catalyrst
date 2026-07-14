# shellcheck shell=bash
# coverage-tour-lib.sh — sourced by coverage-tour.sh; not executable on its own.
# Holds the tour's process management, tcase harness, stub servers, node
# sidecars, fixture writers, and per-command wrappers. The case functions and
# the driver table stay in coverage-tour.sh. Expects the caller's config vars
# (BIN WORK LOGS SREQ LH WS KEY ADDR STUB STUB_PORT DEAD NODE and the fixture
# dirs) to be set before any function here runs.

PIDS=()
cleanup() {
  for p in "${PIDS[@]:-}"; do kill -9 "$p" 2>/dev/null; done
  wait 2>/dev/null
}
trap cleanup EXIT INT TERM

spawn() { "$@" & PIDS+=($!); LAST_PID=$!; }
stop_server() { kill -9 "$SERVER_PID" 2>/dev/null || true; }

wait_for() { # secs cmd... — poll once per second
  local n=$1 i=0; shift
  until "$@"; do i=$((i + 1)); [ "$i" -ge "$n" ] && return 1; sleep 1; done
}
wait_http() { wait_for "${2:-30}" curl -sf -m 2 -o /dev/null "$1"; }
tcp_ok() { python3 -c 'import socket, sys; socket.create_connection(("127.0.0.1", int(sys.argv[1])), 1)' "$1" 2>/dev/null; }
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
post_json() { curl -s -X POST -H 'content-type: application/json' -d "$@"; }
post_jsonf() { curl -sf -X POST -H 'content-type: application/json' -d "$@"; }
jget() { python3 -c 'import json, sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }
linker_bg() { # timeout-secs logfile cmd... -> LINKER_PID
  local s=$1 log=$2; shift 2
  DCL_ONE_SDK_LINKER_TIMEOUT_SECS=$s "$@" > "$log" 2>&1 &
  LINKER_PID=$!; PIDS+=("$LINKER_PID")
}
need_pty() { command -v script > /dev/null || skip "no script(1) for PTY"; }

PASS=0; FAIL=0; SKIP=0; FAILED_CASES=()
declare -A RESULT
# The subshell must NOT sit in a tested context (if/&&/!): that would void its
# set -e and reduce every case to its last command's status.
tcase() { # NN name fn
  local id="$1 $2"
  echo "$id" | grep -qE "$FILTER" || return 0
  local log=$LOGS/$1-$2.log rc=0
  ( set -e; if [ -n "${TOUR_TRACE:-}" ]; then set -x; fi; "$3" ) > "$log" 2>&1
  rc=$?
  if [ "$rc" -eq 0 ]; then
    RESULT[$id]=PASS; PASS=$((PASS + 1)); echo "PASS $id"
  elif [ -e "$LOGS/.skip" ]; then
    rm -f "$LOGS/.skip"; RESULT[$id]=SKIP; SKIP=$((SKIP + 1)); echo "SKIP $id"
  else
    RESULT[$id]=FAIL; FAIL=$((FAIL + 1)); FAILED_CASES+=("$id"); echo "FAIL $id (log: $log)"
  fi
}
skip() { touch "$LOGS/.skip"; echo "skip: $*"; exit 1; }
# set -e ignores a leading `!`, so a `! cmd` assert that goes wrong would not
# fail the case; `fails cmd` counts.
fails() {
  local rc=0
  "$@" || rc=$?
  [ "$rc" -ne 0 ]
}

write_stub() {
  cat > "$WORK/stub.py" <<'PYEOF'
import json, os, re, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]); ADDR = sys.argv[2]
BASE = f"http://127.0.0.1:{PORT}"
LOG = open(sys.argv[3], "a", buffering=1)
CONTENTS_DIR = sys.argv[4]
NO_DEPLOY = {"owner": "0x1111111111111111111111111111111111111111",
             "permissions": {"deployment": {"type": "allow-list", "wallets": []}},
             "summary": {ADDR: [{"permission": "deployment", "world_wide": False}]}}
A_MD = {"name": "a.md", "path": "ai-sdk-context/a.md", "type": "file",
        "download_url": f"{BASE}/dl/a.md"}

class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, fmt, *a): LOG.write(fmt % a + "\n")
    def _send(self, code, body, ctype="application/json"):
        data = body if isinstance(body, bytes) else body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def j(self, obj, code=200): self._send(code, json.dumps(obj))
    def _path(self, verb, drain=True):
        if drain:
            n = int(self.headers.get("Content-Length") or 0)
            if n: self.rfile.read(n)
        p = self.path.split("?")[0]
        LOG.write(f"{verb} {p}\n")
        return p
    def do_GET(self):
        p = self._path("GET", drain=False)
        if p in ("/about", "/content/about"):
            return self.j({"healthy": True, "acceptingUsers": True,
                           "content": {"publicUrl": f"{BASE}/content"},
                           "configurations": {"realmName": "stub"}})
        if re.fullmatch(r"/world/[^/]+/scenes", p):
            return self.j({"scenes": [
                {"entityId": "e1", "parcels": ["5,5"], "baseParcel": "5,5"}]})
        if re.fullmatch(r"/world/(denied|parcels(-denied)?)\.dcl\.eth/permissions", p):
            return self.j(NO_DEPLOY)
        if p == f"/world/parcels.dcl.eth/permissions/deployment/address/{ADDR}/parcels":
            return self.j({"parcels": ["0,0"]})
        if re.fullmatch(rf"/world/(denied|parcels-denied)\.dcl\.eth/permissions/deployment/address/{ADDR}/parcels", p):
            return self.j({"parcels": []})
        if re.fullmatch(r"/world/[^/]+/permissions", p):
            return self.j({"owner": ADDR, "permissions": {
                "deployment": {"type": "allow-list", "wallets": [ADDR]},
                "access": {"type": "unrestricted"},
                "streaming": {"type": "allow-list", "wallets": []}}})
        if p == "/world/refused.dcl.eth/settings":
            return self.j({"error": "no"}, 403)
        if re.fullmatch(r"/world/[^/]+/settings", p):
            return self.j({"settings": {"title": "Stub World"}})
        if p == "/api/root":
            return self.j([A_MD,
                {"name": "sub", "path": "ai-sdk-context/sub", "type": "dir",
                 "url": f"{BASE}/api/sub"}])
        if p == "/api/sub":
            return self.j([
                {"name": "b.md", "path": "ai-sdk-context/sub/b.md", "type": "file",
                 "download_url": f"{BASE}/dl/b.md"}])
        if p == "/api/partial":
            return self.j([A_MD,
                {"name": "gone.md", "path": "ai-sdk-context/gone.md", "type": "file",
                 "download_url": f"{BASE}/dl/gone.md"}])
        if p == "/dl/a.md": return self._send(200, "alpha", "text/plain")
        if p == "/dl/b.md": return self._send(200, "beta", "text/plain")
        m = re.fullmatch(r"/content/contents/([A-Za-z0-9]+)", p)
        if m:
            f = os.path.join(CONTENTS_DIR, m.group(1))
            if os.path.isfile(f):
                return self._send(200, open(f, "rb").read(), "application/octet-stream")
        self._send(404, "no stub rule", "text/plain")
    def do_POST(self):
        p = self._path("POST")
        if p == "/reject/entities":
            return self._send(500, "the deployment pipeline exploded", "text/plain")
        if p == "/content/entities/active":
            return self.j([])
        if p.endswith("/entities"):
            return self.j({"creationTimestamp": 1})
        self._send(404, "no stub rule", "text/plain")
    def do_PUT(self):
        p = self._path("PUT")
        if p == "/world/denied.dcl.eth/settings":
            return self.j({"error": "denied"}, 403)
        if re.fullmatch(r"/world/[^/]+/settings", p):
            return self.j({"ok": True})
        if re.fullmatch(r"/world/[^/]+/permissions/[^/]+/0x[0-9a-fA-F]{40}", p):
            return self.j({"ok": True})
        self._send(404, "no stub rule", "text/plain")
    def do_DELETE(self):
        p = self._path("DELETE")
        if p == "/entities/linkerworld.dcl.eth":
            return self._send(200, "{}")          # upstream-shape world delete OK
        if re.fullmatch(r"/entities/[^/]+", p):
            return self._send(404, "not found", "text/plain")
        if re.fullmatch(r"/world/[^/]+/scenes/[^/]+", p):
            return self._send(200, "")
        if p.endswith("/scenes/9,9"):
            return self._send(404, "No locally published scene at 9,9", "text/plain")
        if re.search(r"/scenes/-?\d+,-?\d+$", p):
            return self._send(200, "{}")
        if re.fullmatch(r"/world/[^/]+/permissions/[^/]+/0x[0-9a-fA-F]{40}", p):
            return self.j({"ok": True})
        self._send(404, "no stub rule", "text/plain")
    def do_HEAD(self):
        LOG.write(f"HEAD {self.path}\n")
        self.send_response(200); self.send_header("Content-Length", "0"); self.end_headers()

ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
PYEOF
}

write_ws_probe() {
  cat > "$WORK/ws-probe.mjs" <<'JSEOF'
// ws-probe.mjs <scene-dir> <url> <seconds> <outfile> [subprotocol] [origin]
import { createRequire } from 'node:module'
import path from 'node:path'
import fs from 'node:fs'
const [sceneDir, url, secs, outfile, subprotocol, origin] = process.argv.slice(2)
const req = createRequire(path.join(path.resolve(sceneDir), 'package.json'))
const WebSocket = req('ws')
const out = fs.createWriteStream(outfile, { flags: 'a' })
const opts = {}
if (subprotocol) opts.protocol = subprotocol
const headers = {}
if (origin) headers.Origin = origin
const ws = new WebSocket(url, subprotocol ? [subprotocol] : [], { headers })
ws.binaryType = 'arraybuffer'
ws.on('open', () => out.write('OPEN\n'))
ws.on('message', (data, isBinary) => {
  if (isBinary) out.write(`BIN ${Buffer.from(data).length} ${Buffer.from(data).toString('hex').slice(0, 120)}\n`)
  else out.write(`TEXT ${data}\n`)
})
ws.on('error', (e) => out.write(`ERROR ${e.message}\n`))
ws.on('close', (c) => out.write(`CLOSE ${c}\n`))
ws.on('unexpected-response', (_r, res) => { out.write(`HTTP ${res.statusCode}\n`); process.exit(0) })
setTimeout(() => { try { ws.close() } catch {} ; setTimeout(() => process.exit(0), 300) }, Number(secs) * 1000)
JSEOF
}

# node signer: EIP-191 personal_sign via the npm scene's ethereum-cryptography
write_signer() {
  cat > "$WORK/personal-sign.mjs" <<'JSEOF'
// personal-sign.mjs <npm-scene-dir> <privkey-hex> <message>
import { createRequire } from 'node:module'
import path from 'node:path'
const [sceneDir, privHex, message] = process.argv.slice(2)
const req = createRequire(path.join(path.resolve(sceneDir), 'package.json'))
const { keccak256 } = req('ethereum-cryptography/keccak')
const secpMod = req('ethereum-cryptography/secp256k1')
const secp = secpMod.secp256k1 ?? secpMod
const msg = Buffer.from(message)
const prefixed = Buffer.concat([
  Buffer.from(`\x19Ethereum Signed Message:\n${msg.length}`), msg])
const hash = keccak256(prefixed)
const priv = Buffer.from(privHex, 'hex')
let sigHex, recovery
if (secp.sign) {
  const sig = secp.sign(hash, priv)
  sigHex = sig.toCompactHex(); recovery = sig.recovery
} else {
  const [sig, rec] = await secpMod.sign(hash, priv, { recovered: true, der: false })
  sigHex = Buffer.from(sig).toString('hex'); recovery = rec
}
process.stdout.write('0x' + sigHex + (27 + recovery).toString(16))
JSEOF
}

sign_message() { # message -> signature on stdout
  "$NODE" "$WORK/personal-sign.mjs" "$NPM_SCENE" "$KEY" "$1"
}

write_min_scene() { # dir parcels [extra-scene-json-tail]
  mkdir -p "$1/bin"
  printf '{"main":"bin/index.js","runtimeVersion":"7","scene":{"parcels":["%s"],"base":"%s"}%s}' \
    "$2" "$2" "${3:-}" > "$1/scene.json"
  echo 'console.log(1);' > "$1/bin/index.js"
}

write_composite() { # path
  mkdir -p "$(dirname "$1")"
  cat > "$1" <<'EOF'
{
  "version": 1,
  "components": [
    {
      "name": "core::Transform",
      "data": {
        "512": {
          "json": {
            "position": { "x": 8, "y": 0, "z": 8 },
            "rotation": { "x": 0, "y": 0, "z": 0, "w": 1 },
            "scale": { "x": 1, "y": 1, "z": 1 },
            "parent": 0
          }
        }
      }
    },
    {
      "name": "core::MeshRenderer",
      "data": {
        "512": { "json": { "mesh": { "$case": "box", "box": { "uvs": [] } } } }
      }
    }
  ]
}
EOF
}

add_script_component() { # composite-path
  python3 - "$1" <<'EOF'
import json, sys
p = sys.argv[1]; c = json.load(open(p))
c["components"].append({"name": "asset-packs::Script",
                        "data": {"512": {"json": {"src": "scripts/x.js"}}}})
json.dump(c, open(p, "w"))
EOF
}

npm_scene_clone() { # dir — bare scene wired to the full npm-scene node_modules
  rm -rf "$1"; mkdir -p "$1/src"
  cp "$SCENE/scene.json" "$SCENE/package.json" "$SCENE/tsconfig.json" "$1/"
  echo 'export function main() {}' > "$1/src/index.ts"
  ln -s "$NPM_SCENE/node_modules" "$1/node_modules"
}

broken_src() { # dir src-code — scene skeleton sharing $SCENE's node_modules
  rm -rf "$1"; mkdir -p "$1/src"
  cp "$SCENE/scene.json" "$SCENE/tsconfig.json" "$1/"
  ln -s "$SCENE/node_modules" "$1/node_modules"
  echo "$2" > "$1/src/index.ts"
}

need_signer() { write_signer; test -d "$NPM_SCENE/node_modules/ethereum-cryptography" || skip "no npm scene for signing"; }
need_npm_scene() { test -d "$NPM_SCENE/node_modules/@dcl/sdk-commands" || skip "${1:-no full npm scene}"; }

setup_fixtures() {
  # Free the fixed tour port range from any leftover of a previous run so a
  # server never connects to a stranger holding its port.
  pkill -9 -f "$BIN start" 2>/dev/null || true
  pkill -9 -f "$(basename "$TUNNEL_BIN")" 2>/dev/null || true
  sleep 1
  write_stub
  write_ws_probe
  mkdir -p "$WORK/stub-contents"
  spawn python3 "$WORK/stub.py" "$STUB_PORT" "$ADDR" "$SREQ" "$WORK/stub-contents"
  wait_http "$STUB/about" 15 || { echo "stub server failed to boot" >&2; exit 1; }
  echo "$KEY" > "$WORK/sign.key"
  printf '\x89PNG\r\n\x1a\n' > "$WORK/thumb.png"
  write_min_scene "$DEPLOY_MIN" "52,-52"
  write_min_scene "$WORLD_MIN" "0,0" ',"worldConfiguration":{"name":"example.dcl.eth"}'
}

start_and_wait() { # port args... (uses $STARTDIR; server log goes to $LOGS/.start-$port.log)
  local port=$1; shift
  ( cd "$STARTDIR" && exec "$BIN" start -p "$port" "$@" ) > "$LOGS/.start-$port.log" 2>&1 &
  local pid=$!
  PIDS+=("$pid"); SERVER_PID=$pid
  # Guard against connecting to a leftover server on the fixed port: if our own
  # process exited (bind refused), fail loudly instead of probing a stranger.
  local i=0
  until curl -sf -m 2 -o /dev/null "$LH:$port/about"; do
    kill -0 "$pid" 2>/dev/null || { echo "start on $port exited early:"; tail -n 5 "$LOGS/.start-$port.log"; return 1; }
    i=$((i + 1)); [ "$i" -ge 120 ] && return 1; sleep 1
  done
}
start_static() { start_and_wait "$1" --ci --no-watch --no-asset-bundles "${@:2}"; }

kdeploy() { # dir extra-flags... — key-signed CI deploy
  DCL_PRIVATE_KEY=$KEY "$BIN" deploy --dir "$1" --skip-build --ci "${@:2}"
}
kunpub() { DCL_PRIVATE_KEY=$KEY "$BIN" unpublish "$@"; }
wset() { "$BIN" world settings set "$1" --target-content "$STUB" --sign-key "$WORK/sign.key" "${@:2}"; }
wperm() { "$BIN" world permissions "$@" --target-content "$STUB" --sign-key "$WORK/sign.key"; }
