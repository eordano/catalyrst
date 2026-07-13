#!/usr/bin/env bash
set -euo pipefail

ABGEN_SRC="${1:?usage: $0 <crates/catalyrst-abgen> <out-dir>}"
OUT="${2:?}"

[ -d "$ABGEN_SRC/src" ] || { echo "no crate src at: $ABGEN_SRC" >&2; exit 1; }
[ -d "$ABGEN_SRC/export-overlay" ] || { echo "no export-overlay at: $ABGEN_SRC" >&2; exit 1; }
mkdir -p "$OUT"
[ -z "$(ls -A "$OUT")" ] || { echo "out dir not empty: $OUT" >&2; exit 1; }

ROOT_TREES=(pipeline site harness template scripts)

mkdir -p "$OUT/crate"
cp -R "$ABGEN_SRC"/. "$OUT/crate/"
cp -R "$(dirname "$ABGEN_SRC")/dcl-contents" "$OUT/crate/dcl-contents"
chmod -R u+w "$OUT"
rm -rf "$OUT/crate/dcl-contents/target"
for t in "${ROOT_TREES[@]}" export-overlay; do rm -rf "$OUT/crate/$t"; done
rm -f  "$OUT/crate/src/bin/catalyrst-abgen.rs"
rm -f  "$OUT/crate/.gitignore"
rm -f  "$OUT/crate/abgen-compare.json"
rm -rf "$OUT/crate/third_party/draco_decoder/third_party/draco/build"
rm -rf "$OUT/crate/target"
rm -rf "$OUT/crate/runs" "$OUT/crate/wasm-poc/target"
rm -rf "$OUT/crate/kernel-ptx/target"
rm -f  "$OUT/crate/OPERATIONS.md"
rm -f  "$OUT/crate/third_party/crunch/README-upstream.md"
rm -f  "$OUT/crate/third_party/draco_decoder/third_party/draco/BUILDING.md"
rm -f  "$OUT/crate/third_party/draco_decoder/third_party/draco/CMAKE.md"
rm -f  "$OUT/crate/third_party/draco_decoder/third_party/draco/CONTRIBUTING.md"
rm -f  "$OUT/crate/third_party/draco_decoder/third_party/draco/README.md"

mv "$OUT/crate/src/bin/abgen.rs" "$OUT/crate/src/bin/abgen-build.rs"

for t in "${ROOT_TREES[@]}"; do cp -R "$ABGEN_SRC/$t" "$OUT/$t"; done
rm -f "$OUT/site/wasm/abgen_poc.wasm"

for p in "$ABGEN_SRC/export-overlay"/* "$ABGEN_SRC/export-overlay"/.[!.]*; do
  [ -e "$p" ] || continue
  base="$(basename "$p")"
  if [ "$base" = "crate" ]; then
    cp -R "$p"/. "$OUT/crate/"
  else
    cp -R "$p" "$OUT/$base"
  fi
done

chmod -R u+w "$OUT"
find "$OUT" -name __pycache__ -type d -prune -exec rm -rf {} +
find "$OUT" -name '.DS_Store' -type f -delete
find "$OUT" -name '*.o' -type f -delete

mapfile -t _pkg_refs < <(grep -rIl 'catalyrst-abgen' "$OUT" 2>/dev/null || true)
for f in "${_pkg_refs[@]}"; do
  [ -n "$f" ] && sed -i 's/catalyrst-abgen/abgen/g' "$f"
done

echo "assembled abgen-rs tree at $OUT ($(find "$OUT" -type f | wc -l) files)"
