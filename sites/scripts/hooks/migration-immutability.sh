#!/usr/bin/env bash
# Applied migrations are immutable. Once a migration file is committed it may
# already have run against a live DB (content, squid, per-crate), so editing,
# deleting, or renaming it silently corrupts every deployment that applied the
# old bytes -- an incident in both directions. A change goes in a NEW migration
# with the next number instead. A file that was genuinely never applied anywhere
# can be reshaped with ALLOW_MIGRATION_EDIT=1 (loud, deliberate).
#
# Tracked at catalyrst/sites/scripts/hooks/migration-immutability.sh; run from
# the pre-commit hook. Fast path: exits 0 when no committed migration is staged
# for modify/delete/rename.
set -euo pipefail

blocked=()
while IFS=$'\t' read -r _status path _new; do
  case "$path" in
    catalyrst/crates/*/migrations/*.sql|catalyrst/contracts/squid/db/migrations/*.sql) ;;
    *) continue ;;
  esac
  git cat-file -e "HEAD:$path" 2>/dev/null || continue
  blocked+=("$path")
done < <(git diff --cached --name-status --diff-filter=MDR)

(( ${#blocked[@]} == 0 )) && exit 0

if [[ "${ALLOW_MIGRATION_EDIT:-0}" == 1 ]]; then
  echo "pre-commit: migration-immutability OVERRIDDEN (ALLOW_MIGRATION_EDIT=1):" >&2
  printf '  %s\n' "${blocked[@]}" >&2
  exit 0
fi

echo "pre-commit: refusing to modify already-committed migration(s) -- they may have run on a live DB. Add a NEW migration with the next number instead:" >&2
printf '  %s\n' "${blocked[@]}" >&2
echo "  (a file never applied anywhere: ALLOW_MIGRATION_EDIT=1 git commit ...)" >&2
exit 1
