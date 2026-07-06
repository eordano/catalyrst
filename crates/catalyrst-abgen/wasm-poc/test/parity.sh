#!/usr/bin/env bash
# Byte-parity gate: rebuilds the wasm module and the native binaries from the
# same tree, converts every fixture on both sides across windows/mac/webgl,
# and sha256-compares each produced artifact. Exits nonzero on any mismatch.
# Decoder rule: the native production default stays turbojpeg; the gate
# exports ABGEN_JPEG_GLB_9C=1 so native decodes GLB JPEGs via the vendored
# libjpeg9c exactly like wasm (no dlopen there, always libjpeg9c).
# Env knobs: PARITY_SKIP_BUILD=1 reuses existing binaries, PARITY_WORK=dir.
set -euo pipefail
POC="$(cd "$(dirname "$0")/.." && pwd)"
CRATE="$(cd "$POC/.." && pwd)"
WS="$(cd "$CRATE/../.." && pwd)"

if [ -z "${PARITY_IN_SHELL:-}" ]; then
  exec nix develop "path:$POC/toolchain" --command \
    env PARITY_IN_SHELL=1 bash "$POC/test/parity.sh" "$@"
fi

export ABGEN_ROOT="$CRATE"
export ABGEN_REAL_TEXTURES=1
export ABGEN_JPEG_GLB_9C=1

WORK="${PARITY_WORK:-$POC/target/parity}"
WOUT="$WORK/wasm"
NOUT="$WORK/native"
rm -rf "$WORK"
mkdir -p "$WOUT" "$NOUT"

if [ "${PARITY_SKIP_BUILD:-0}" != 1 ]; then
  bash "$POC/build.sh"
  nix develop "$WS" -c cargo build --release -p catalyrst-abgen --bin abgen --bin abgen-lod
fi

python3 "$POC/test/make-fixtures.py" "$(command -v draco_encoder || true)"

FIX="$POC/test/fixtures"
ABUILD="$WS/target/release/abgen"
ALOD="$WS/target/release/abgen-lod"
PLATFORMS="windows mac webgl"
FIXTURES="jpeg-quad normal-quad draco-quad gamma-quad transform-quad tangent-quad multimat-quad scene-lod"

fails=0
rows=()

row() { rows+=("$1|$2|$3|$4|$5|$6"); }

pairs() {
  sed -n '/"ev":"file-start"/s/.*"file":"\([^"]*\)".*"bundle":"\([^"]*\)".*/\1 \2/p' "$1"
}

copy_lod() {
  local t="$1" sid="$2" plat="$3" ndir="$4"
  cp "$t/lodout/$sid/LOD/1/${sid}_1_${plat}" "$ndir/${sid}_1_${plat}" &&
    cp "$t/lodout/$sid/LOD/1/${sid}_1_${plat}.br" "$ndir/${sid}_1_${plat}.br" &&
    cp "$t/lodout/$sid/LOD.manifest.json" "$ndir/LOD.manifest.json" &&
    cp "$t/lodout/$sid/LOD.manifest.json.br" "$ndir/LOD.manifest.json.br"
}

for fx in $FIXTURES; do
  if [ "$fx" = scene-lod ]; then
    files=("$FIX/scene-lod/model.glb" "$FIX/scene-lod/scene.json")
    etype=scene lodflag=--lod=1
  else
    files=("$FIX/$fx.glb")
    etype=wearable lodflag=--lod=0
  fi
  if [ ! -f "${files[0]}" ]; then
    row "$fx" all "(fixture)" - - MISSING
    fails=$((fails + 1))
    continue
  fi

  for plat in $PLATFORMS; do
    wdir="$WOUT/$fx/$plat" ndir="$NOUT/$fx/$plat"
    mkdir -p "$wdir" "$ndir"
    log="$wdir/log.txt"
    if ! node "$POC/test/headless.mjs" "$wdir" "$plat" '' "$lodflag" "${files[@]}" >"$log" 2>&1; then
      sed 's/^/  wasm: /' "$log" >&2
      row "$fx" "$plat" "(wasm-run)" - - FAIL
      fails=$((fails + 1))
      continue
    fi
    sid="$(sed -n 's/.*"entityHash":"\([0-9a-f]*\)".*/\1/p' "$log" | head -1)"
    bundle="$(sed -n '/"ev":"file-start"/s/.*"bundle":"\([^"]*\)".*/\1/p' "$log" | head -1)"
    if [ -z "$sid" ] || [ -z "$bundle" ]; then
      row "$fx" "$plat" "(wasm-parse)" - - FAIL
      fails=$((fails + 1))
      continue
    fi

    nlog="$ndir/log.txt"
    if ! "$ABUILD" "${files[0]}" "$bundle" "$sid" "$ndir/$bundle" \
        --source-file "$(basename "${files[0]}")" --entity-type "$etype" \
        --magenta-missing >"$nlog" 2>&1; then
      sed 's/^/  native: /' "$nlog" >&2
    fi

    if [ "$fx" = scene-lod ] && [ "$plat" != webgl ]; then
      # Stage the atlas input as {sid}_1.glb: the file stem becomes the lodgen
      # root_name, which is baked into the emitted GLB and hence the bundle.
      t="$ndir/lodwork"
      mkdir -p "$t"
      cp "${files[0]}" "$t/${sid}_1.glb"
      if "$ALOD" atlas -i "$t/${sid}_1.glb" -o "$t/atlased.glb" \
            --max-size 1024 --padding 2 --atlas-fixed >>"$nlog" 2>&1 \
         && "$ALOD" bundle "$t/atlased.glb" --entity "$sid" --level 1 \
            --platform "$plat" --base 0,0 --parcels "0,0;1,0" \
            --out "$t/lodout" >>"$nlog" 2>&1; then
        cp "$t/lodout/$sid/LOD/1/${sid}_1_${plat}" "$ndir/${sid}_1_${plat}"
        cp "$t/lodout/$sid/LOD/1/${sid}_1_${plat}.br" "$ndir/${sid}_1_${plat}.br"
        cp "$t/lodout/$sid/LOD.manifest.json" "$ndir/LOD.manifest.json"
        cp "$t/lodout/$sid/LOD.manifest.json.br" "$ndir/LOD.manifest.json.br"
      else
        sed 's/^/  native-lod: /' "$nlog" >&2
      fi
    fi

    for f in "$wdir"/*; do
      name="$(basename "$f")"
      case "$name" in log.txt|manifest.json) continue ;; esac
      wsha="$(sha256sum "$f" | cut -c1-12)"
      if [ -f "$ndir/$name" ]; then
        nsha="$(sha256sum "$ndir/$name" | cut -c1-12)"
        if [ "$wsha" = "$nsha" ]; then st=OK; else
          st=DIFF
          fails=$((fails + 1))
        fi
      else
        nsha=-
        st=MISSING
        fails=$((fails + 1))
      fi
      row "$fx" "$plat" "$name" "$wsha" "$nsha" "$st"
    done

    # The native abgen flow emits no manifest, so manifest.json is a
    # wasm-only structural check (bundle set + "dcl"), never a byte compare.
    got="$(sed -n 's/.*"files":\[\([^]]*\)\].*/\1/p' "$wdir/manifest.json" 2>/dev/null || true)"
    want="\"$bundle\",\"dcl\""
    if [ "$got" = "$want" ]; then st=OK; else
      st=DIFF
      fails=$((fails + 1))
    fi
    row "$fx" "$plat" manifest.json n/a n/a "$st"
  done
done

# LOD-lane + recovery fixtures: multi-file uploads with per-file native
# equivalents; the LOD chains stage every intermediate under the {sid}_1
# stem because from_glb_bytes takes the root name from the file stem.
for fx in dense-decimate-lod crop-overhang-lod placements-iss-lod badjpeg-pair; do
  case "$fx" in
    dense-decimate-lod)
      files=("$FIX/$fx/model.glb" "$FIX/$fx/scene.json")
      etype=scene plats="windows mac" wflags="--lod=1 --tri-cap=500" ;;
    crop-overhang-lod)
      files=("$FIX/$fx/model.glb" "$FIX/$fx/scene.json")
      etype=scene plats="windows mac" wflags="--lod=1 --crop=1" ;;
    placements-iss-lod)
      files=("$FIX/$fx/tri.glb" "$FIX/$fx/cube.glb" "$FIX/$fx/scene.json"
             "$FIX/$fx/fixture_InitialSceneState.json")
      etype=scene plats="windows mac" wflags="--lod=1" ;;
    badjpeg-pair)
      files=("$FIX/$fx/good.glb" "$FIX/$fx/bad.glb")
      etype=wearable plats="$PLATFORMS" wflags="--lod=0" ;;
  esac
  if [ ! -f "${files[0]}" ]; then
    row "$fx" all "(fixture)" - - MISSING
    fails=$((fails + 1))
    continue
  fi

  for plat in $plats; do
    wdir="$WOUT/$fx/$plat" ndir="$NOUT/$fx/$plat"
    mkdir -p "$wdir" "$ndir"
    log="$wdir/log.txt"
    if ! node "$POC/test/headless.mjs" "$wdir" "$plat" '' $wflags "${files[@]}" >"$log" 2>&1; then
      sed 's/^/  wasm: /' "$log" >&2
      row "$fx" "$plat" "(wasm-run)" - - FAIL
      fails=$((fails + 1))
      continue
    fi
    sid="$(sed -n 's/.*"entityHash":"\([0-9a-f]*\)".*/\1/p' "$log" | head -1)"
    if [ -z "$sid" ]; then
      row "$fx" "$plat" "(wasm-parse)" - - FAIL
      fails=$((fails + 1))
      continue
    fi
    nlog="$ndir/log.txt"
    : >"$nlog"

    while read -r src bundle; do
      if ! "$ABUILD" "$FIX/$fx/$src" "$bundle" "$sid" "$ndir/$bundle" \
          --source-file "$src" --entity-type "$etype" \
          --magenta-missing >>"$nlog" 2>&1; then
        sed 's/^/  native: /' "$nlog" >&2
      fi
    done < <(pairs "$log")

    t="$ndir/lodwork"
    mkdir -p "$t"
    case "$fx" in
      dense-decimate-lod)
        mkdir -p "$t/a" "$t/s"
        cp "${files[0]}" "$t/${sid}_1.glb"
        if "$ALOD" atlas -i "$t/${sid}_1.glb" -o "$t/a/${sid}_1.glb" \
              --max-size 1024 --padding 2 --atlas-fixed >>"$nlog" 2>&1 \
           && "$ALOD" simplify -i "$t/a/${sid}_1.glb" -o "$t/s/${sid}_1.glb" \
              --simplifier meshopt --ratio 1 --tri-cap 500 >>"$nlog" 2>&1 \
           && "$ALOD" bundle "$t/s/${sid}_1.glb" --entity "$sid" --level 1 \
              --platform "$plat" --base 0,0 --parcels "0,0;1,0" \
              --out "$t/lodout" >>"$nlog" 2>&1; then
          copy_lod "$t" "$sid" "$plat" "$ndir"
        else
          sed 's/^/  native-lod: /' "$nlog" >&2
        fi
        if sed -n '/"ev":"lod-simplify"/p' "$log" | grep -q '"trisBefore":2048'; then
          row "$fx" "$plat" "lod-simplify(ran)" n/a n/a OK
        else
          row "$fx" "$plat" "lod-simplify(ran)" n/a n/a FAIL
          fails=$((fails + 1))
        fi
        ;;
      crop-overhang-lod)
        mkdir -p "$t/a"
        cp "${files[0]}" "$t/${sid}_1.glb"
        if "$ALOD" atlas -i "$t/${sid}_1.glb" -o "$t/a/${sid}_1.glb" \
              --crop-base 0,0 --crop-parcels "0,0;1,0" \
              --max-size 1024 --padding 2 --atlas-fixed >>"$nlog" 2>&1 \
           && "$ALOD" bundle "$t/a/${sid}_1.glb" --entity "$sid" --level 1 \
              --platform "$plat" --base 0,0 --parcels "0,0;1,0" \
              --out "$t/lodout" >>"$nlog" 2>&1; then
          copy_lod "$t" "$sid" "$plat" "$ndir"
        else
          sed 's/^/  native-lod: /' "$nlog" >&2
        fi
        cropev="$(sed -n '/"ev":"lod-crop"/p' "$log" | head -1)"
        if [ -n "$cropev" ] && ! echo "$cropev" | grep -q '"trisClipped":0,' \
           && ! echo "$cropev" | grep -q '"trisDropped":0,'; then
          row "$fx" "$plat" "lod-crop(clips+drops)" n/a n/a OK
        else
          row "$fx" "$plat" "lod-crop(clips+drops)" n/a n/a FAIL
          fails=$((fails + 1))
        fi
        ;;
      placements-iss-lod)
        staged="$t/staged"
        mkdir -p "$staged" "$t/m"
        for f in "${files[@]}"; do
          cp "$f" "$staged/$(sha256sum "$f" | cut -d' ' -f1)"
        done
        python3 - "$sid" "$t/entity.json" "${files[@]}" <<'PYEOF'
import hashlib, json, sys
sid, out, *paths = sys.argv[1:]
content = [{"file": p.rsplit("/", 1)[-1],
            "hash": hashlib.sha256(open(p, "rb").read()).hexdigest()}
           for p in paths]
doc = {"id": sid, "type": "scene", "pointers": [], "content": content,
       "metadata": {"scene": {"base": "0,0", "parcels": ["0,0", "1,0"]}}}
open(out, "w").write(json.dumps(doc))
PYEOF
        if "$ALOD" assemble --entity-json "$t/entity.json" \
              --iss "$FIX/$fx/fixture_InitialSceneState.json" \
              --cache "$staged" --catalyst http://127.0.0.1:9 \
              --no-crop --max-size 1024 --padding 2 --atlas-fixed --level 1 \
              -o "$t/m/merged.glb" >>"$nlog" 2>&1 \
           && "$ALOD" bundle "$t/m/merged.glb" --entity "$sid" --level 1 \
              --platform "$plat" --base 0,0 --parcels "0,0;1,0" \
              --out "$t/lodout" >>"$nlog" 2>&1; then
          copy_lod "$t" "$sid" "$plat" "$ndir"
        else
          sed 's/^/  native-lod: /' "$nlog" >&2
        fi
        if grep -q '"msg":"placements: 3 from ISS"' "$log"; then
          row "$fx" "$plat" "iss(3 placements)" n/a n/a OK
        else
          row "$fx" "$plat" "iss(3 placements)" n/a n/a FAIL
          fails=$((fails + 1))
        fi
        ;;
      badjpeg-pair)
        # The corrupt JPEG must fail per-image, not per-run: pre-recovery the
        # 9c longjmp trapped the whole conversion here. Native degrades the
        # failed decode to the deterministic missing-texture placeholder, so
        # bad.glb still converts and byte-compares; the run must stay
        # trap-free and complete both files.
        if grep -q '^FATAL' "$log"; then
          row "$fx" "$plat" "(no-trap)" n/a n/a FAIL
          fails=$((fails + 1))
        else
          row "$fx" "$plat" "(no-trap)" n/a n/a OK
        fi
        if sed -n '/"ev":"file-done"/p' "$log" | grep -q '"file":"bad.glb"'; then
          row "$fx" "$plat" "bad.glb(recovered)" n/a n/a OK
        else
          row "$fx" "$plat" "bad.glb(recovered)" n/a n/a FAIL
          fails=$((fails + 1))
        fi
        if grep -q '"exitCode":0' "$wdir/manifest.json" 2>/dev/null; then
          row "$fx" "$plat" "manifest(exitCode 0)" n/a n/a OK
        else
          row "$fx" "$plat" "manifest(exitCode 0)" n/a n/a FAIL
          fails=$((fails + 1))
        fi
        ;;
    esac

    for f in "$wdir"/*; do
      name="$(basename "$f")"
      case "$name" in log.txt|manifest.json) continue ;; esac
      wsha="$(sha256sum "$f" | cut -c1-12)"
      if [ -f "$ndir/$name" ]; then
        nsha="$(sha256sum "$ndir/$name" | cut -c1-12)"
        if [ "$wsha" = "$nsha" ]; then st=OK; else
          st=DIFF
          fails=$((fails + 1))
        fi
      else
        nsha=-
        st=MISSING
        fails=$((fails + 1))
      fi
      row "$fx" "$plat" "$name" "$wsha" "$nsha" "$st"
    done

    want=""
    for b in $(sed -n '/"ev":"file-done"/s/.*"bundle":"\([^"]*\)".*/\1/p' "$log" | sort -u); do
      want="$want\"$b\","
    done
    want="${want}\"dcl\""
    got="$(sed -n 's/.*"files":\[\([^]]*\)\].*/\1/p' "$wdir/manifest.json" 2>/dev/null || true)"
    if [ "$got" = "$want" ]; then st=OK; else
      st=DIFF
      fails=$((fails + 1))
    fi
    row "$fx" "$plat" manifest.json n/a n/a "$st"
  done
done

printf '%-15s %-8s %-76s %-12s %-12s %s\n' FIXTURE PLATFORM ARTIFACT WASM NATIVE STATUS
for r in "${rows[@]}"; do
  IFS='|' read -r a b c d e s <<<"$r"
  printf '%-15s %-8s %-76s %-12s %-12s %s\n' "$a" "$b" "$c" "$d" "$e" "$s"
done

if [ "$fails" -ne 0 ]; then
  echo "PARITY: FAIL ($fails bad rows)"
  exit 1
fi
echo "PARITY: OK (${#rows[@]} rows)"
