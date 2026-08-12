#!/usr/bin/env bash
# Assemble the standalone dcl-one-sdk workspace tree from ~/one source.
#
#   dcl-one-sdk-standalone-assemble.sh <one-root> <out-dir>
#
# The published dcl-one-sdk is a self-contained cargo workspace: the crate plus
# the five in-workspace path deps it needs, under crates/, with the workspace
# scaffold (Cargo.toml / Cargo.lock / flake / clippy / LICENSE / README /
# .gitignore) coming from the crate's export-overlay/. The overlay Cargo.lock
# pins the whole tree so `nix build` / `cargo build --locked` are reproducible;
# regenerate it (cargo generate-lockfile in an assembled tree) whenever a member
# crate's deps move. Only git-TRACKED crate files are copied (dev-only harness
# dirs excluded), so the output is reproducible from HEAD.
set -euo pipefail

ONE="${1:?usage: $0 <one-root> <out-dir>}"
OUT="${2:?}"
CRATE_DIR="$ONE/catalyrst/crates/dcl-one-sdk"
OVERLAY="$CRATE_DIR/export-overlay"

[ -d "$CRATE_DIR/src" ] || { echo "no dcl-one-sdk crate at $CRATE_DIR" >&2; exit 1; }
[ -d "$OVERLAY" ]       || { echo "no export-overlay at $OVERLAY" >&2; exit 1; }
mkdir -p "$OUT"
[ -z "$(ls -A "$OUT" 2>/dev/null)" ] || { echo "out dir not empty: $OUT" >&2; exit 1; }

# The workspace members, in dependency order (must match export-overlay/Cargo.toml).
MEMBERS=(catalyrst-envcfg catalyrst-types catalyrst-hashing catalyrst-crypto catalyrst-preview-tunnel catalyrst-testgate dcl-one-sdk)

# Per-crate path prefixes NOT published (dev harnesses / gated UI drivers that
# reference private tooling; they are not part of the shipped toolchain).
# export-overlay/ is this crate's OWN publication scaffold — it becomes the tree
# ROOT via the overlay rsync below, so it must never also ship nested under
# crates/dcl-one-sdk/ (that would republish the generator into its own output).
EXCLUDE_RE='^(scripts/|tests/data_layer_ui\.rs$|\.claude/|export-overlay/)'

mkdir -p "$OUT/crates"
for m in "${MEMBERS[@]}"; do
  src="$ONE/catalyrst/crates/$m"
  [ -d "$src" ] || { echo "missing member crate: $src" >&2; exit 1; }
  ( cd "$src" && git ls-files ) | grep -vE "$EXCLUDE_RE" > "$OUT/.files.$m"
  [ -s "$OUT/.files.$m" ] || { echo "member $m matched no tracked files?" >&2; exit 1; }
  mkdir -p "$OUT/crates/$m"
  rsync -rlpt --files-from="$OUT/.files.$m" "$src/" "$OUT/crates/$m/"
  rm -f "$OUT/.files.$m"
done

# Overlay the standalone workspace scaffold on top (workspace root files;
# a trailing-slash rsync copies dotfiles like .gitignore too).
rsync -rlpt --exclude='.git' "$OVERLAY"/ "$OUT"/

echo "assembled dcl-one-sdk standalone workspace -> $OUT ($(cd "$OUT" && find crates -type f | wc -l) crate files)"
