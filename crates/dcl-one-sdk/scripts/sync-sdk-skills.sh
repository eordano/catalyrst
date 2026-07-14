#!/usr/bin/env bash
# Sync skills/ with decentraland/sdk-skills.
#
# Upstream-owned skill directories are mirrored exactly (including deletions);
# the LOCAL_ONLY skills are ours alone and never touched. build.rs embeds
# whatever sits in skills/ at compile time, so a sync followed by a build is
# the whole update.
#
#   scripts/sync-sdk-skills.sh [path-to-sdk-skills-checkout]
set -euo pipefail

upstream="${1:-$HOME/github.com-decentraland/sdk-skills}"
dest="$(cd "$(dirname "$0")/.." && pwd)/skills"
LOCAL_ONLY=(migrate-smart-items-to-code)

[ -f "$upstream/LICENSE" ] || { echo "no LICENSE at $upstream — not an sdk-skills checkout" >&2; exit 1; }

ours() {
  local d
  for d in "${LOCAL_ONLY[@]}"; do [ "$1" = "$d" ] && return 0; done
  return 1
}

synced=0
for dir in "$upstream"/*/; do
  name="$(basename "$dir")"
  [ -f "$dir/SKILL.md" ] || continue
  ours "$name" && { echo "skip $name (local skill shadows an upstream name)" >&2; continue; }
  rsync -a --delete --exclude '.DS_Store' "$dir" "$dest/$name/"
  synced=$((synced + 1))
done

for dir in "$dest"/*/; do
  name="$(basename "$dir")"
  ours "$name" && continue
  [ -f "$upstream/$name/SKILL.md" ] || { rm -r "$dir"; echo "removed $name (gone upstream)"; }
done

cp "$upstream/LICENSE" "$dest/LICENSE.sdk-skills"
commit="$(git -C "$upstream" rev-parse HEAD 2>/dev/null || echo unknown)"
cat >"$dest/UPSTREAM.md" <<EOF
Skills synced from https://github.com/decentraland/sdk-skills
(Apache-2.0, see LICENSE.sdk-skills) at commit $commit.

Local-only skills, not from upstream: ${LOCAL_ONLY[*]}.

Re-sync with scripts/sync-sdk-skills.sh; build.rs embeds this directory into
the binary, so follow a sync with a build.
EOF
echo "synced $synced skills from $upstream @ $commit"
