#!/usr/bin/env bash
set -euo pipefail
bad=()
while IFS= read -r f; do
  case "$f" in *.py) bad+=("$f"); continue ;; esac
  if git show ":$f" | head -c 64 | grep -qE '^#!.*python'; then bad+=("$f"); fi
done < <(git diff --cached --name-only --diff-filter=A)
(( ${#bad[@]} == 0 )) && exit 0
echo "pre-commit: new python is not accepted, write it in bash or Rust (rig/rust):" >&2
printf '  %s\n' "${bad[@]}" >&2
exit 1
