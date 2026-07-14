BRIDGE_CRATE=bridge_protocol

GENERATED_DIR_REL="ui3/src/generated"

EDITOR_BUS_COPIES=(
  "bevy-explorer/editor-scene/src/generated/editor-bus.ts"
  "tools/scene-editor-mcp/src/generated/editor-bus.ts"
)

TS_CRATES_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Which crates the gate runs is DERIVED from [package.metadata.generated] in
# each emitting crate's own Cargo.toml -- never a list kept here, which is how a
# crate silently falls off. Lazy, so sourcing this file stays node-free for
# consumers that only want the constants above.
load_generated_crates() {
  local script="$TS_CRATES_ROOT/sites/scripts/generated-artefacts.mts"
  local bridge_crates=()
  mapfile -t CATALYRST_CRATES < <(node "$script" --list catalyrst-pkgs) || return 1
  mapfile -t CATALYRST_TS_CRATES < <(node "$script" --list catalyrst-ts) || return 1
  mapfile -t OPENAPI_SPECS < <(node "$script" --list catalyrst-openapi) || return 1
  mapfile -t bridge_crates < <(node "$script" --list bridge) || return 1
  if [[ ${#CATALYRST_CRATES[@]} -eq 0 || ${#CATALYRST_TS_CRATES[@]} -eq 0 ||
    ${#OPENAPI_SPECS[@]} -eq 0 || ${#bridge_crates[@]} -ne 1 ]]; then
    echo "ts-crates: no crate declares [package.metadata.generated] -- refusing to run an empty gate" >&2
    return 1
  fi
  BRIDGE_CRATE="${bridge_crates[0]}"
}

# The check and the regenerate must land on the SAME compiler or the gate can
# never be cleared. Pin one shell per area, derived from the tree being checked:
# catalyrst honours its rust-toolchain.toml through the #ci shell, the bridge
# needs the bevy nightly + rust-src AND the native libs its dev-deps build
# against (alsa/udev on Linux) -- the default shell, not the toolchain-only
# #rust subset. Do NOT reintroduce a $DCL_SHELL fallback -- rig/config.sh
# exports it as the bevy shell, so honouring it here silently builds catalyrst
# on the wrong toolchain.
ts_rs_shell_run() {
  local area="$1" cmd="$2" flake
  case "$area" in
    catalyrst) flake="$TS_CRATES_ROOT#ci" ;;
    bridge) flake="$TS_CRATES_ROOT/bevy-explorer" ;;
    *)
      echo "ts_rs_shell_run: unknown area $area (expected: catalyrst, bridge)" >&2
      return 2
      ;;
  esac
  if ! command -v nix >/dev/null 2>&1; then
    echo "ts_rs_shell_run: nix is not on PATH, so the $area toolchain cannot be pinned" >&2
    return 127
  fi
  nix develop "$flake" --command bash -c "$cmd"
}

assemble_editor_bus() {
  local src_dir="$1" out_file="$2" f
  {
    echo "// GENERATED from bridge_protocol::editor via ts-rs + sites/scripts/gen-ts-types.sh. Do not edit."
    for f in $(LC_ALL=C ls "$src_dir"/*.ts | LC_ALL=C sort); do
      [[ "$(basename "$f")" == "constants.ts" ]] && continue
      echo ""
      grep -v -e '^// This file was generated' -e '^import type ' "$f" | sed '/^$/d'
    done
    echo ""
    cat "$src_dir/constants.ts"
  } >"$out_file"
}
