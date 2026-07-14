#!/usr/bin/env bash
# 1200-line cap on source files: a staged .rs/.ts/.tsx/.nix file over the cap
# fails the commit unless the commit shrinks it, so the files already over
# the line can be worked down but nothing grows past it. Generated and
# vendored trees are exempt.
set -euo pipefail

cap=${LINE_CAP:-1200}
over=()
while IFS= read -r f; do
  case "$f" in
    *.rs|*.ts|*.tsx|*.nix) ;;
    *) continue ;;
  esac
  case "$f" in
    *generated/*|*third-party/*|*third_party/*|*node_modules/*|*vendor/*|*.d.ts|*.react-router/*|*/deploy/env-contract.nix) continue ;;
  esac
  now=$(git show ":$f" | wc -l)
  (( now > cap )) || continue
  was=$(git show "HEAD:$f" 2>/dev/null | wc -l || true)
  (( now > ${was:-0} )) || continue
  over+=("$f: $was -> $now")
done < <(git diff --cached --name-only --diff-filter=AM)

(( ${#over[@]} == 0 )) && exit 0
echo "pre-commit: line cap $cap exceeded by a growing file. Split it (foo.rs -> foo/mod.rs + submodules; sites route internals -> app/components|lib):" >&2
printf '  %s\n' "${over[@]}" >&2
exit 1
