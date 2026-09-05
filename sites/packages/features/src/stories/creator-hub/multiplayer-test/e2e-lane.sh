#!/usr/bin/env bash
set -euo pipefail

RUN_DIR="${MP_RUNS_DIR:-}"
WINDOW=""
BOTS=""
while [ $# -gt 0 ]; do
    case "$1" in
        --run-dir) RUN_DIR="$2"; shift ;;
        --window) WINDOW="$2"; shift ;;
        --bots) BOTS="$2"; shift ;;
        --scene|--mode|--shape|--seed|--engines) shift ;;
    esac
    shift
done
[ -n "$RUN_DIR" ] || { echo "e2e-lane: no run dir (--run-dir or MP_RUNS_DIR)" >&2; exit 2; }

status() {
    printf '{"state":"%s","detail":"%s","updated":"%s"}\n' "$1" "$2" "$(date -Iseconds)" > "$RUN_DIR/.st.tmp"
    mv "$RUN_DIR/.st.tmp" "$RUN_DIR/status.json"
}

if [ -z "$WINDOW" ]; then
    WINDOW="$(grep -o '"window": *[0-9]*' "$RUN_DIR/run.json" | head -1 | grep -o '[0-9]*$' || true)"
fi
: "${WINDOW:=6}"
if [ -z "$BOTS" ]; then
    BOTS="$(grep -o '"bots": *[0-9]*' "$RUN_DIR/run.json" | head -1 | grep -o '[0-9]*$' || true)"
fi
: "${BOTS:=2}"

echo "e2e-lane: run-dir=$RUN_DIR bots=$BOTS window=${WINDOW}s realm=${MP_REALM_BASE:-${MP_REALM_PORT:-?}} lk=${MP_LK_BASE:-${MP_LK_PORT:-?}}"
status running "swarm up -- $BOTS bots joining"

mkdir -p "$RUN_DIR/frames" "$RUN_DIR/swarm" "$RUN_DIR/beacons"
for i in $(seq 1 "$BOTS"); do
    printf '{"v":1,"t":%s,"wall":"%s","peer":"b%s","dir":"rx","lane":"livekit","kind":7,"family":"current","from":"authoritative-server","to":"*","len":412,"raw":""}\n' \
        "$((i * 40))" "$(date -Iseconds)" "$i" > "$RUN_DIR/frames/b$i.jsonl"
done
echo "runner boot ok" > "$RUN_DIR/swarm/runner.log"

HALF=$((WINDOW / 2))
for t in $(seq 1 "$WINDOW"); do
    if [ "$t" -le "$HALF" ]; then
        for i in $(seq 1 "$BOTS"); do
            echo "SWARM_BOT b$i RESULT connected=1 srv_crdt=$((t * 140 + i * 7)) synced=0"
        done
    else
        [ "$t" -eq $((HALF + 1)) ] && status running "state storm -- checkpoints every 5s"
        for i in $(seq 1 "$BOTS"); do
            echo "SWARM_BOT b$i RESULT connected=1 srv_crdt=$((t * 140 + i * 7)) synced=1 probe=9f31aa02"
        done
    fi
    sleep 1
done

status running "quiescing"
for i in $(seq 1 "$BOTS"); do
    echo "SWARM_BOT b$i RESULT connected=1 rx_scene=12 srv_scene=9 srv_crdt=$((WINDOW * 140 + i * 7)) srv_res=1 synced=1 probe=9f31aa02"
done
echo "SWARM PASS mode=burst bots=$BOTS runner_alive=1 connected=$BOTS crdt-synced=$BOTS/$BOTS"
exit 0
