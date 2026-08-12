#!/usr/bin/env bash
set -euo pipefail

SITES="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$SITES/../.." && pwd)"
# shellcheck source=ts-crates.sh
source "$SITES/scripts/ts-crates.sh"
GENERATED="$ROOT/$GENERATED_DIR_REL"
load_generated_crates
node "$SITES/scripts/generated-artefacts.mts" --write

catalyrst_cmd="set -e; export TS_RS_EXPORT_DIR='$GENERATED/catalyst'; cd '$ROOT/catalyrst'"
for crate in "${CATALYRST_CRATES[@]}"; do
  features=""
  for ts_crate in "${CATALYRST_TS_CRATES[@]}"; do
    if [[ "$ts_crate" == "$crate" ]]; then features="--features ts"; fi
  done
  catalyrst_cmd+="; cargo test $features -p '$crate' --lib export_bindings"
done
ts_rs_shell_run catalyrst "$catalyrst_cmd"
node "$SITES/scripts/gen-openapi-ts.mts" "$GENERATED/catalyst/openapi" "${OPENAPI_SPECS[@]}"

ts_rs_shell_run bridge "set -e; export TS_RS_EXPORT_DIR='$GENERATED'; cd '$ROOT/bevy-explorer'; \
cargo test --features ts -p '$BRIDGE_CRATE' --lib export_bindings"

assemble_editor_bus "$GENERATED/editor-bus" "$GENERATED/editor-bus.ts"
rm -rf "$GENERATED/editor-bus"
for dest in "${EDITOR_BUS_COPIES[@]}"; do
  mkdir -p "$(dirname "$ROOT/$dest")"
  cp "$GENERATED/editor-bus.ts" "$ROOT/$dest"
done

echo "generated TS types -> $GENERATED (editor-bus.ts mirrored to: ${EDITOR_BUS_COPIES[*]})"
