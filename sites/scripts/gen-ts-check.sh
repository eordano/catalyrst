#!/usr/bin/env bash
set -euo pipefail

SITES="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$SITES/../.." && pwd)"
# shellcheck source=ts-crates.sh
source "$SITES/scripts/ts-crates.sh"
COMMITTED="$ROOT/$GENERATED_DIR_REL"

check_catalyrst=0
check_bridge=0
if [[ $# -eq 0 ]]; then
  check_catalyrst=1
  check_bridge=1
fi
for area in "$@"; do
  case "$area" in
    catalyrst) check_catalyrst=1 ;;
    bridge) check_bridge=1 ;;
    *)
      echo "unknown area $area (expected: catalyrst, bridge)" >&2
      exit 2
      ;;
  esac
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail_toolchain() {
  echo "" >&2
  echo "ts-rs regeneration FAILED TO BUILD -- this is a toolchain problem, not a" >&2
  echo "drift verdict." >&2
  exit 3
}

node "$SITES/scripts/generated-artefacts.mts" --check
load_generated_crates

if [[ $check_catalyrst -eq 1 ]]; then
  crate_dirs=()
  for crate in "${CATALYRST_CRATES[@]}"; do
    crate_dirs+=("$ROOT/catalyrst/crates/$crate")
  done
  node "$SITES/scripts/ts-serde-soundness.mts" "${crate_dirs[@]}"
fi

if [[ $check_catalyrst -eq 1 ]]; then
  pkg_flags=""
  feature_list=""
  for crate in "${CATALYRST_CRATES[@]}"; do
    pkg_flags+=" -p $crate"
  done
  for crate in "${CATALYRST_TS_CRATES[@]}"; do
    feature_list+="${feature_list:+,}$crate/ts"
  done
  ts_rs_shell_run catalyrst "set -e; export TS_RS_EXPORT_DIR='$TMP/catalyst'; cd '$ROOT/catalyrst'; \
cargo test --features '$feature_list'$pkg_flags export_bindings" || fail_toolchain
  spec_set_rc=0
  node "$SITES/scripts/gen-openapi-ts.mts" "$TMP/catalyst/openapi" \
    "${OPENAPI_SPECS[@]}" || spec_set_rc=$?
  # 3 is the generator's spec-set verdict, not a build failure: report it as drift.
  [[ $spec_set_rc -eq 0 || $spec_set_rc -eq 3 ]] || fail_toolchain
fi
if [[ $check_bridge -eq 1 ]]; then
  ts_rs_shell_run bridge "set -e; export TS_RS_EXPORT_DIR='$TMP'; cd '$ROOT/bevy-explorer'; \
cargo test --features ts -p $BRIDGE_CRATE export_bindings" || fail_toolchain
fi

fail=0
if [[ $check_catalyrst -eq 1 ]]; then
  [[ ${spec_set_rc:-0} -eq 0 ]] || fail=1
  diff -r -x 'README.md' "$TMP/catalyst" "$COMMITTED/catalyst" || fail=1
fi
if [[ $check_bridge -eq 1 ]]; then
  diff -r -x 'README.md' "$TMP/bridge" "$COMMITTED/bridge" || fail=1
  diff -r -x 'README.md' "$TMP/webbridge" "$COMMITTED/webbridge" || fail=1
  assemble_editor_bus "$TMP/editor-bus" "$TMP/editor-bus.ts"
  diff "$TMP/editor-bus.ts" "$COMMITTED/editor-bus.ts" || fail=1
  for dest in "${EDITOR_BUS_COPIES[@]}"; do
    diff "$TMP/editor-bus.ts" "$ROOT/$dest" || fail=1
  done
fi

if [[ $fail -eq 0 ]]; then
  echo "OK: generated TS types match $COMMITTED"
else
  echo "" >&2
  echo "DRIFT: the committed generated types are stale (see diff above)." >&2
  echo "Regenerate them with:  npm run gen:types  (in catalyrst/sites/)" >&2
  exit 1
fi
