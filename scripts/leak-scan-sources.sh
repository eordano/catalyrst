#!/usr/bin/env bash
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$HERE/export-sanitize-gate.sh"

DIRS=(
  "$HOME/projects/catalyrst"
  "$HOME/projects/abgen"
  "$HOME/projects/dcl-react-ui"
  "$HOME/projects/decentraland-automation-rig"
  "$HOME/projects/opengpinstancer"
  "$HOME/projects/sandboxed-agents"
  "$HOME/projects/unitedav"
  "$HOME/projects/harness-verify-unity"
  "$HOME/projects/dclstudios"
)

fail=0
for d in "${DIRS[@]}"; do
  [ -d "$d" ] || { echo "skip (missing): $d"; continue; }
  extra=""
  case "$d" in
    */unitedav) extra="_win,build-test,build-gpu,build-win" ;;
  esac
  if ! GATE_EXTRA_EXCLUDE_DIRS="$extra" bash "$GATE" "$d"; then fail=1; fi
done

if [ "$fail" != 0 ]; then
  echo "leak-scan: FINDINGS above — sanitize sources before any export/publish" >&2
  exit 1
fi
echo "leak-scan: all export-feeding source dirs clean"
