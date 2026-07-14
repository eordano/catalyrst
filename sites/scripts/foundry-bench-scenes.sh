#!/usr/bin/env bash
# foundry-bench-scenes.sh — one command from built scene workspaces to
# ingest-ready bench evidence. Per scene it serves the local creator preview,
# puts a headless bevy client in it through the already-running dcl-rig sway
# session, runs the dcl-scene-bots manifest against the live client, and
# leaves what the harness wrote — snapshot.json, run.log (the tee'd runner
# stdout the ingest parses), shots/ — plus a NOTES.md naming the exact launch
# and the exact ingest command, in a fresh evidence directory whose basename
# becomes the stable trajectory/report ids.
#
#   scripts/foundry-bench-scenes.sh [scene ...] [options]
#
#   scenes        default: relay-gardens echo-duel caravan-ledger
#   --skip-build  serve the scene's existing bin/index.js as-is
#   --ingest      run foundry:ingest-bench per staged run (DB write; without
#                 this flag the script writes no database anywhere)
#   --db <url>    postgres url for --ingest (default $FOUNDRY_DATABASE_URL)
#   --run-tag <t> evidence dir suffix (default: epoch seconds). A fresh tag is
#                 a fresh run; reusing one REPLACES that run on ingest.
#
# A harness verdict of FAIL still stages: the run happened and the report is
# the place that says how it went. Exit 0 when every scene staged, 1 when any
# scene could not produce evidence (refused, no snapshot, or infra failure).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SITES_DIR="$(dirname "$HERE")"

: "${FOUNDRY_BENCH_EVIDENCE_ROOT:=$HOME/foundry-bench-evidence}"
: "${DCLBOTS_DIR:=$HOME/projects/dcl-scene-bots}"
: "${DCL_SCENE_WORKSPACES:=$HOME/workspaces}"
: "${DCL_RIG_PORT:=5919}"
: "${DCL_BEVY_BIN:=$HOME/one/bevy-explorer/target/release/decentra-bevy}"
: "${DCL_BEVY_ENV:=$HOME/.cache/dcl-shell/bevy-explorer.env}"
: "${DCL_BEVY_READY_TIMEOUT:=180}"

# Fixed per-scene ports so concurrent agents on this box can see at a glance
# whose listener is whose; scan_port-style drifting would defeat that.
ports_for() {
  case "$1" in
    relay-gardens)  REALM_PORT=8021; MCP_PORT=8131 ;;
    echo-duel)      REALM_PORT=8022; MCP_PORT=8132 ;;
    caravan-ledger) REALM_PORT=8023; MCP_PORT=8133 ;;
    *) echo "foundry-bench-scenes: no port assignment for scene '$1' — add it to ports_for()" >&2; return 1 ;;
  esac
}

SCENES=()
SKIP_BUILD=0
INGEST=0
DB_URL="${FOUNDRY_DATABASE_URL:-}"
RUN_TAG="$(date +%s)"
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --ingest) INGEST=1; shift ;;
    --db) DB_URL="$2"; shift 2 ;;
    --run-tag) RUN_TAG="$2"; shift 2 ;;
    --*) echo "foundry-bench-scenes: unknown flag $1 (see the header of this script)" >&2; exit 1 ;;
    *) SCENES+=("$1"); shift ;;
  esac
done
[ ${#SCENES[@]} -gt 0 ] || SCENES=(relay-gardens echo-duel caravan-ledger)

log() { printf '[bench-scenes] %s\n' "$*" >&2; }

# The rig compositor runs bwrap'd with its own /tmp/.X11-unix; swaymsg exec
# is the one door in, and the swaymsg beside the compositor binary is the one
# that speaks its IPC version.
RIG_RT="/run/user/$(id -u)/dcl-rig-$DCL_RIG_PORT"
RIG_TMP="/tmp/dcl-rig-$DCL_RIG_PORT"
rig_resolve() {
  RIG_SOCK="$(ls -t "$RIG_RT"/sway-ipc.*.sock 2>/dev/null | head -1)" || true
  if [ -z "${RIG_SOCK:-}" ]; then
    log "no sway IPC socket under $RIG_RT — bring the display up first: dcl-rig up $DCL_RIG_PORT"
    return 1
  fi
  local base pid exe
  base="${RIG_SOCK##*/}"; base="${base%.sock}"; pid="${base##*.}"
  exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null)" || true
  RIG_SWAYMSG="${exe%/*}/swaymsg"
  if [ ! -x "$RIG_SWAYMSG" ]; then
    log "no swaymsg beside the rig compositor ($exe) — is the rig session on :$DCL_RIG_PORT alive? dcl-rig status"
    return 1
  fi
  RIG_DISPLAY=":$(ls "$RIG_TMP/x11-private" 2>/dev/null | sed -n 's/^X\([0-9]\+\)$/\1/p' | sort -rn | head -1)"
  if [ "$RIG_DISPLAY" = ":" ]; then
    log "no Xwayland socket under $RIG_TMP/x11-private — the rig is up but X never started; dcl-rig down && dcl-rig up $DCL_RIG_PORT"
    return 1
  fi
  if ! SWAYSOCK="$RIG_SOCK" timeout 5 "$RIG_SWAYMSG" -t get_version >/dev/null 2>&1; then
    log "sway IPC on $RIG_SOCK is not answering — dcl-rig down && dcl-rig up $DCL_RIG_PORT"
    return 1
  fi
}

port_free_or_die() {
  local p="$1" owner
  if ss -ltnH "sport = :$p" 2>/dev/null | grep -q LISTEN; then
    owner="$(ss -ltnHp "sport = :$p" 2>/dev/null | grep -oP 'users:\(\("\K[^"]+' | head -1)" || true
    log "port $p is already taken by '${owner:-another process}' — stop it or override the assignment in ports_for()"
    return 1
  fi
}

wait_http() {
  local url="$1" secs="$2"
  local end=$((SECONDS + secs))
  until curl -sf -o /dev/null "$url"; do
    [ $SECONDS -lt $end ] || return 1
    sleep 1
  done
}

# Ready means the explorer answers MCP and names its realm — the same fact
# run.py's wrong_world gate reads, so the harness never burns its episode on a
# client that is still loading.
wait_explorer_ready() {
  local url="$1"
  DCLBOTS_DIR="$DCLBOTS_DIR" python3 - "$url" "$DCL_BEVY_READY_TIMEOUT" <<'PY'
import os, sys, time
sys.path.insert(0, os.environ["DCLBOTS_DIR"])
from dclbots import mcp
url, deadline, last = sys.argv[1], time.time() + float(sys.argv[2]), "never tried"
while time.time() < deadline:
    try:
        client = mcp.connect(url, timeout=5)
        realm = (client.scene_state().get("realm") or "").strip()
        if realm:
            print(f"explorer ready in realm {realm!r}")
            sys.exit(0)
        last = "connected, no realm reported yet"
    except Exception as exc:
        last = str(exc).splitlines()[0]
    time.sleep(3)
print(f"explorer never became ready: {last}", file=sys.stderr)
sys.exit(1)
PY
}

# Both halves are killed by unique command-line pattern: the preview server
# forks an npx→node tree whose pgid is not $!, and the client is a grandchild
# of the rig compositor — a pid is the wrong handle for either.
REALM_PATTERN=""
BEVY_PATTERN=""
teardown_scene() {
  if [ -n "$BEVY_PATTERN" ]; then pkill -f "$BEVY_PATTERN" 2>/dev/null || true; BEVY_PATTERN=""; fi
  if [ -n "$REALM_PATTERN" ]; then pkill -f "$REALM_PATTERN" 2>/dev/null || true; REALM_PATTERN=""; fi
}
trap teardown_scene EXIT

run_scene() {
  local slug="$1"
  local scene_dir="$DCL_SCENE_WORKSPACES/$slug-scene"
  local manifest="$HERE/bench/$slug.json"
  local evdir="$FOUNDRY_BENCH_EVIDENCE_ROOT/$slug-$RUN_TAG"

  [ -d "$scene_dir" ] || { log "$slug: no scene workspace at $scene_dir"; return 1; }
  [ -f "$manifest" ] || { log "$slug: no manifest at $manifest"; return 1; }
  ports_for "$slug" || return 1
  port_free_or_die "$REALM_PORT" || return 1
  port_free_or_die "$MCP_PORT" || return 1
  mkdir -p "$evdir"

  if [ "$SKIP_BUILD" -eq 0 ]; then
    log "$slug: building"
    (cd "$scene_dir" && npm run build) >"$evdir/build.log" 2>&1 \
      || { log "$slug: build failed — see $evdir/build.log"; return 1; }
  fi

  log "$slug: serving preview on :$REALM_PORT"
  REALM_PATTERN="sdk-commands start --no-browser --port $REALM_PORT"
  (cd "$scene_dir" && exec npx @dcl/sdk-commands start --no-browser --port "$REALM_PORT") \
    >"$evdir/sdk-server.log" 2>&1 &
  wait_http "http://127.0.0.1:$REALM_PORT/about" 90 \
    || { log "$slug: preview never answered /about on :$REALM_PORT — see $evdir/sdk-server.log"; return 1; }

  BEVY_PATTERN="mcp-port $MCP_PORT"
  cat >"$evdir/launch-bevy.sh" <<LAUNCH
#!/usr/bin/env bash
set -u
export DISPLAY=$RIG_DISPLAY
source $DCL_BEVY_ENV
exec $DCL_BEVY_BIN \\
  --server http://127.0.0.1:$REALM_PORT --mcp --mcp-port $MCP_PORT
LAUNCH
  log "$slug: launching bevy into rig :$DCL_RIG_PORT (display $RIG_DISPLAY, MCP :$MCP_PORT)"
  # sway's exec joins its arguments and hands them to `sh -c`, so the command
  # must travel as ONE string — a `bash -c` wrapper here degenerates to a bare
  # `bash` and the client never starts.
  SWAYSOCK="$RIG_SOCK" "$RIG_SWAYMSG" exec -- \
    "bash '$evdir/launch-bevy.sh' >'$evdir/bevy.log' 2>&1" >/dev/null
  wait_explorer_ready "http://127.0.0.1:$MCP_PORT" \
    || { log "$slug: explorer never became ready — see $evdir/bevy.log"; return 1; }

  log "$slug: running the harness (stdout tee'd to $evdir/run.log)"
  local harness_exit=0
  (cd "$DCLBOTS_DIR" && python3 -m dclbots.run "$manifest" --url "http://127.0.0.1:$MCP_PORT" --out "$evdir") \
    2>&1 | tee "$evdir/run.log" || harness_exit=$?

  teardown_scene

  if [ ! -f "$evdir/snapshot.json" ]; then
    log "$slug: no snapshot.json — the run was refused or died (exit $harness_exit); nothing to ingest"
    return 1
  fi

  local ingest_cmd="npm run foundry:ingest-bench -- $evdir --manifest $manifest --exit-code $harness_exit"
  cat >"$evdir/NOTES.md" <<NOTES
# $slug-$RUN_TAG — $(date -u +%Y-%m-%dT%H:%M:%SZ)

Staged by scripts/foundry-bench-scenes.sh. Harness exit $harness_exit
(0 pass, 1 a check failed, per-check verdicts in run.log).

- scene workspace: $scene_dir
- manifest: $manifest
- realm: local preview on :$REALM_PORT (LocalPreview), sdk-server.log
- client: $DCL_BEVY_BIN
  sha256 $(sha256sum "$DCL_BEVY_BIN" | cut -d' ' -f1)
  rig :$DCL_RIG_PORT display $RIG_DISPLAY, MCP :$MCP_PORT, bevy.log
- harness: $DCLBOTS_DIR @ $(git -C "$DCLBOTS_DIR" rev-parse --short HEAD 2>/dev/null || echo unversioned)
- ingest (the only DB write, run it from catalyrst/sites with foundry env):
  $ingest_cmd
NOTES

  if [ "$INGEST" -eq 1 ]; then
    log "$slug: ingesting"
    (cd "$SITES_DIR" && npx tsx scripts/foundry-ingest-bench.mts "$evdir" \
      --manifest "$manifest" --exit-code "$harness_exit" ${DB_URL:+--db "$DB_URL"}) || return 1
  else
    log "$slug: staged — to record it: (cd $SITES_DIR && $ingest_cmd)"
  fi
  log "$slug: evidence at $evdir"
}

rig_resolve || exit 1
command -v python3 >/dev/null || { log "python3 is required for the harness"; exit 1; }
[ -x "$DCL_BEVY_BIN" ] || { log "no explorer binary at $DCL_BEVY_BIN — build bevy-explorer or set DCL_BEVY_BIN"; exit 1; }
[ -f "$DCL_BEVY_ENV" ] || { log "no FHS env at $DCL_BEVY_ENV — open a dcl-shell once to generate it, or set DCL_BEVY_ENV"; exit 1; }
[ -d "$DCLBOTS_DIR/dclbots" ] || { log "no dcl-scene-bots checkout at $DCLBOTS_DIR"; exit 1; }

FAILED=()
for slug in "${SCENES[@]}"; do
  run_scene "$slug" || { FAILED+=("$slug"); teardown_scene; }
done

if [ ${#FAILED[@]} -gt 0 ]; then
  log "no evidence staged for: ${FAILED[*]}"
  exit 1
fi
log "all ${#SCENES[@]} scene(s) staged under $FOUNDRY_BENCH_EVIDENCE_ROOT (tag $RUN_TAG)"
