#!/usr/bin/env bash
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
MANIFEST="${TEST_COUNT_MANIFEST:-$ROOT/tests-manifest.tsv}"

usage() {
  cat <<'EOF'
test-count-gate.sh [--list] [crate ...]

Runs the test targets declared in tests-manifest.tsv and fails when a target
reports fewer passing tests than the manifest requires, or produces no test
binary at all. This is the backstop for a suite that compiles to zero tests:
arming the skip gates makes an unrun test fail, but a suite that cargo omits
(required-features), or a file whose #[test]s were deleted, still reports green.

  --list        print the manifest and exit
  crate ...     restrict the run to these crates

Exit:
  0   every selected row met its floor
  1   a target failed, or passed fewer tests than the manifest requires
  2   the gate could not do its job: no row was selected, or a row names a
      crate or target cargo cannot resolve. Never confuse this with 0 - it
      means the counts below were never checked.

Env:
  ALLOW_SKIPPED_INTEGRATION   must NOT be set; the gate refuses to run with it
  CARGO                       cargo binary (default: cargo)
EOF
}

[[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] && { usage; exit 0; }

if [[ -n "${ALLOW_SKIPPED_INTEGRATION:-}" ]]; then
  echo "test-count-gate: refusing to run with ALLOW_SKIPPED_INTEGRATION set;" >&2
  echo "  the gate exists to prove the suites actually ran." >&2
  exit 2
fi

if [[ ! -f "$MANIFEST" ]]; then
  echo "test-count-gate: no manifest at $MANIFEST" >&2
  exit 2
fi

if [[ "${1:-}" == "--list" ]]; then
  grep -v '^\s*#' "$MANIFEST" | grep -v '^\s*$'
  exit 0
fi

CARGO="${CARGO:-cargo}"
declare -a ONLY=("$@")

want_crate() {
  [[ ${#ONLY[@]} -eq 0 ]] && return 0
  local c
  for c in "${ONLY[@]}"; do [[ "$c" == "$1" ]] && return 0; done
  return 1
}

fail=0
unresolved=0
checked=0

while IFS=$'\t' read -r crate target min_passed features; do
  [[ -z "${crate:-}" || "${crate:0:1}" == "#" ]] && continue
  want_crate "$crate" || continue
  checked=$((checked + 1))

  declare -a args=(test -p "$crate")
  if [[ "$target" == "lib" ]]; then
    args+=(--lib)
  else
    args+=(--test "$target")
  fi
  [[ -n "${features:-}" && "$features" != "-" ]] && args+=(--features "$features")

  echo "== $crate / $target (expect >= $min_passed passing)"
  out="$("$CARGO" "${args[@]}" -- --test-threads=1 2>&1 </dev/null)"
  rc=$?
  echo "$out" | grep -E '^(test result|error|thread .* panicked|SKIPPED)' | sed 's/^/   /'

  if [[ $rc -ne 0 ]]; then
    if echo "$out" | grep -qE 'no test target named|did not match any packages'; then
      echo "   UNRESOLVED: this row names a crate or target that does not exist"
      unresolved=$((unresolved + 1))
    else
      echo "   FAIL: cargo exited $rc"
      fail=1
    fi
    continue
  fi

  summary="$(echo "$out" | grep -E '^test result:' | tail -1)"
  if [[ -z "$summary" ]]; then
    echo "   FAIL: no test binary ran - the target was omitted or produced no tests"
    fail=1
    continue
  fi

  passed="$(echo "$summary" | sed -nE 's/.*ok\. ([0-9]+) passed.*/\1/p')"
  ignored="$(echo "$summary" | sed -nE 's/.*; ([0-9]+) ignored.*/\1/p')"
  passed="${passed:-0}"
  ignored="${ignored:-0}"

  if [[ "$passed" -lt "$min_passed" ]]; then
    echo "   FAIL: $passed passed, manifest requires >= $min_passed ($ignored ignored)"
    fail=1
  else
    echo "   ok: $passed passed ($ignored ignored)"
  fi
done < "$MANIFEST"

if [[ $checked -eq 0 ]]; then
  echo "test-count-gate: manifest matched no targets" >&2
  exit 2
fi

if [[ $unresolved -ne 0 ]]; then
  echo "test-count-gate: manifest matched no targets for $unresolved row(s)" >&2
  echo "  a row names a crate or target cargo cannot resolve, so its floor was never checked." >&2
  exit 2
fi

exit $fail
