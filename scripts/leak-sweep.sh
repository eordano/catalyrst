#!/usr/bin/env bash
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PATTERNS=(
  '/home/dcl'
  '/Users/'
  'home/usr'
  'persist/colmena'
  '100\.64\.'
  '192\.168\.'
  '\bsaturn\b'
  '\bshade\b'
  '\bv16\b'
  '\bmars\b'
  '\binterconnected\b'
  'credentials/'
  'BEGIN[ A-Z]*PRIVATE KEY'
  'dcl\.one'
  'dcl\.tools'
  '\bforgejo\b'
  '\bumbrella\b'
  '\blorebook\b'
  '~/one\b'
  'one-way export'
  'export-(abgen-rs|catalyrst|sanitize-gate|automation-bridge)\.sh'
)

REPOS=(catalyrst abgen-rs dcl-react-ui dcl-automation-rig unitedav)
KNOWN_SHAS=()

while [ $# -gt 0 ]; do
  case "$1" in
    --repos) read -r -a REPOS <<< "$2"; shift 2 ;;
    --shas)  read -r -a KNOWN_SHAS <<< "$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

GH_EXPORT_KEY="${GH_EXPORT_KEY:-/home/dcl/one/credentials/gh-eordano}"
export GIT_SSH_COMMAND="ssh -F /dev/null -i $GH_EXPORT_KEY -o IdentitiesOnly=yes -o ConnectTimeout=12"
export GIT_TERMINAL_PROMPT=0

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

retry() { local i; for i in 1 2 3 4 5; do "$@" && return 0; sleep 3; done; return 1; }

scan_blob() { # <label> <text-file>  -> prints hits, returns 1 if any
  local label="$1" file="$2" p hits found=0
  for p in "${PATTERNS[@]}"; do
    hits="$(grep -naE -e "$p" "$file" 2>/dev/null | grep -vF '100.64.0.0/10' | head -8 || true)"
    if [ -n "$hits" ]; then
      printf '  %-58s [%s]\n' "$label" "$p"
      printf '%s\n' "$hits" | sed 's/^/      /'
      found=1
    fi
  done
  return $found
}

FAIL=0
for repo in "${REPOS[@]}"; do
  echo "=== $repo ==="
  url="https://github.com/eordano/$repo.git"
  mir="$TMP/$repo.git"
  if ! retry git clone --quiet --mirror "$url" "$mir"; then
    echo "  UNREACHABLE (does not exist publicly, or DNS failed 5x) — skipping"
    echo
    continue
  fi

  mapfile -t refs < <(git -C "$mir" for-each-ref --format='%(refname)')
  defbranch="$(git -C "$mir" symbolic-ref --quiet HEAD 2>/dev/null || echo refs/heads/main)"
  n_main="$(git -C "$mir" rev-list --count "$defbranch" 2>/dev/null || echo '?')"
  n_pull="$(printf '%s\n' "${refs[@]}" | grep -c '^refs/pull/' || true)"
  echo "  refs=${#refs[@]} (pull/*=$n_pull)  ${defbranch#refs/heads/}=$n_main commit(s)"
  if [ "$n_pull" -gt 0 ]; then
    echo "  NOTE: $n_pull PR ref(s) present — these serve their blobs forever, immune to force-push."
  fi

  for ref in "${refs[@]}"; do
    while IFS=$'\t' read -r mode type sha path; do
      [ "$type" = blob ] || continue
      case "$path" in
        *.png|*.jpg|*.jpeg|*.ico|*.woff*|*.ttf|*.wasm|*.gz|*.zip|*.glb|*.bin) continue ;;
      esac
      git -C "$mir" cat-file -p "$sha" 2>/dev/null > "$TMP/blob"
      scan_blob "${ref#refs/}:$path" "$TMP/blob" || FAIL=1
    done < <(git -C "$mir" ls-tree -r --long "$ref" 2>/dev/null | awk '{print $1"\t"$2"\t"$3"\t"$5}')
  done

  git -C "$mir" log --all --format='commit %H%n%an <%ae>%n%B' 2>/dev/null > "$TMP/msgs"
  scan_blob "$repo: commit messages" "$TMP/msgs" || FAIL=1
  echo
done

if [ "${#KNOWN_SHAS[@]}" -gt 0 ]; then
  echo "=== known dangling shas ==="
  for sha in "${KNOWN_SHAS[@]}"; do
    code="$(curl -s -m 15 -o /dev/null -w '%{http_code}' "https://github.com/eordano/catalyrst/commit/$sha" 2>/dev/null || echo 000)"
    echo "  $sha -> HTTP $code $([ "$code" = 200 ] && echo '(STILL PUBLICLY FETCHABLE)')"
    [ "$code" = 200 ] && FAIL=1
  done
  echo
fi

if [ -n "${GH_API_TOKEN:-}" ]; then
  echo "=== account fork/repo enumeration ==="
  curl -s -m 20 -H "Authorization: Bearer $GH_API_TOKEN" \
    'https://api.github.com/users/eordano/repos?per_page=100&sort=pushed' 2>/dev/null \
    | grep -oE '"full_name": *"[^"]+"' | sed 's/.*: *"/  /; s/"$//'
  echo "  (compare against REPOS[]; investigate any fork with non-upstream branches)"
else
  echo "NOTE: GH_API_TOKEN unset — account-wide fork enumeration SKIPPED."
  echo "      The audit caught eordano/Pulse (published exploit PoCs) only"
  echo "      via this phase. Set GH_API_TOKEN to cover surprise repos/forks/branches."
fi

if [ "$FAIL" != 0 ]; then
  echo "LEAK SWEEP: HITS FOUND (see above)."
  exit 1
fi
echo "LEAK SWEEP: clean across ${REPOS[*]}"
