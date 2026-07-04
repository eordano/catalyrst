#!/usr/bin/env bash
# Assemble the standalone abgen source tree from crates/catalyrst-abgen.
#
#   abgen-standalone-assemble.sh <crates/catalyrst-abgen> <out-dir>
#
# Used by the flake's abgen-standalone-src derivation. Only coreutils+bash,
# so it runs inside nix sandboxes.
#
# crates/catalyrst-abgen is the ONE authored home of abgen: rust crate
# (src/, examples/, third_party/, shader/, testdata/, build.rs, Cargo.toml)
# plus the compare pipeline/site/harness/template/scripts trees, plus
# export-overlay/ = the only export-specific content (standalone repo
# metadata + the crate files that intentionally differ standalone).
#
# Mapping:
#   crates/catalyrst-abgen/**           -> crate/**   (the rust crate) EXCEPT
#     - pipeline/ site/ harness/ template/ scripts/   -> repo root instead
#     - export-overlay/                               -> repo root (last, wins)
#     - src/bin/catalyrst-abgen.rs        EXCLUDED (prod server bin: folded
#                                         registry + workspace coupling)
#     - .gitignore                        EXCLUDED (root overlay has the
#                                         published one)
#     - src/bin/abgen.rs                  RENAMED to src/bin/abgen-build.rs
#                                         (standalone name for the converter;
#                                         the standalone `abgen` bin is the
#                                         JIT server, from the overlay)
#   export-overlay/{README.md,KNOWN-GAPS.md,LICENSE,.gitignore,flake.nix,
#                   flake.lock,Cargo.toml,Cargo.lock}  -> repo root
#   export-overlay/crate/**                            -> crate/** (standalone
#     Cargo.toml features/deps, feature-gated abcdn/ incl. vendored
#     content_db/range shims, stub regen.rs, the standalone server bin)
set -euo pipefail

ABGEN_SRC="${1:?usage: $0 <crates/catalyrst-abgen> <out-dir>}"
OUT="${2:?}"

[ -d "$ABGEN_SRC/src" ] || { echo "no crate src at: $ABGEN_SRC" >&2; exit 1; }
[ -d "$ABGEN_SRC/export-overlay" ] || { echo "no export-overlay at: $ABGEN_SRC" >&2; exit 1; }
mkdir -p "$OUT"
[ -z "$(ls -A "$OUT")" ] || { echo "out dir not empty: $OUT" >&2; exit 1; }

ROOT_TREES=(pipeline site harness template scripts)

# 1) crate/ = the crate minus root trees, overlay dir and prod-only pieces
mkdir -p "$OUT/crate"
cp -R "$ABGEN_SRC"/. "$OUT/crate/"
chmod -R u+w "$OUT"
for t in "${ROOT_TREES[@]}" export-overlay; do rm -rf "$OUT/crate/$t"; done
rm -f  "$OUT/crate/src/bin/catalyrst-abgen.rs"
rm -f  "$OUT/crate/.gitignore"
rm -rf "$OUT/crate/third_party/draco_decoder/third_party/draco/build"
rm -rf "$OUT/crate/target"

# 2) bin rename: prod converter bin `abgen` publishes as `abgen-build`
mv "$OUT/crate/src/bin/abgen.rs" "$OUT/crate/src/bin/abgen-build.rs"

# 3) repo root trees
for t in "${ROOT_TREES[@]}"; do cp -R "$ABGEN_SRC/$t" "$OUT/$t"; done

# 4) export overlay (root metadata + intentionally-divergent crate files)
for p in "$ABGEN_SRC/export-overlay"/* "$ABGEN_SRC/export-overlay"/.[!.]*; do
  [ -e "$p" ] || continue
  base="$(basename "$p")"
  if [ "$base" = "crate" ]; then
    cp -R "$p"/. "$OUT/crate/"
  else
    cp -R "$p" "$OUT/$base"
  fi
done

# 5) scrub non-source litter
chmod -R u+w "$OUT"
find "$OUT" -name __pycache__ -type d -prune -exec rm -rf {} +
find "$OUT" -name '.DS_Store' -type f -delete

echo "assembled abgen-rs tree at $OUT ($(find "$OUT" -type f | wc -l) files)"
