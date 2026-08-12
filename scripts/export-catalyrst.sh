#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CAT="$(dirname "$HERE")"

TARGET="$HOME/projects/catalyrst"
PUSH=0
BUMP=1
while [ $# -gt 0 ]; do
  case "$1" in
    --push) PUSH=1; shift ;;
    --no-bump) BUMP=0; shift ;;
    --target) TARGET="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

run_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    ( cd "$CAT" && cargo "$@" )
  else
    ( cd "$CAT" && nix develop "$CAT" --command cargo "$@" )
  fi
}

# Gate cargo: ALWAYS the devshell, never host cargo — the devshell resolves
# rust-toolchain.toml exactly like CI does, so preflight verdicts match CI.
gate_cargo() {
  ( cd "$CAT" && nix develop "$CAT" --command cargo "$@" )
}

# Export preflight. The release suite that greens a version (fmt/clippy/tests)
# never nix-builds the packages, which is how v0.15.0 shipped a sites bundle
# that could not build at all (stale npmDepsHash; then un-copied ui3 files).
# Fail the export before it can ship packaging its consumers cannot build.
# SKIP_NIX_PREFLIGHT=1 opts out (emergency exports only).
nix_preflight() {
  if [ "${SKIP_NIX_PREFLIGHT:-0}" = 1 ]; then
    echo "preflight: SKIPPED (SKIP_NIX_PREFLIGHT=1)" >&2
    return 0
  fi
  echo "preflight: cargo fmt --check (pinned toolchain)"
  gate_cargo fmt --all -- --check \
    || { echo "preflight FAILED: rustfmt drift" >&2; exit 1; }
  echo "preflight: cargo clippy -D warnings (pinned toolchain)"
  gate_cargo clippy --workspace --all-targets -- -D warnings \
    || { echo "preflight FAILED: clippy" >&2; exit 1; }
  echo "preflight: nix build sites#sites"
  nix build "$CAT/sites#sites" --no-link \
    --extra-experimental-features 'nix-command flakes' \
    || { echo "preflight FAILED: the sites package does not nix-build" >&2; exit 1; }
  echo "preflight: force-eval every main-flake package drvPath"
  nix eval "$CAT#packages.x86_64-linux" \
    --apply 'ps: map (n: (builtins.getAttr n ps).drvPath) (builtins.attrNames ps)' \
    --extra-experimental-features 'nix-command flakes' >/dev/null \
    || { echo "preflight FAILED: main flake packages do not evaluate" >&2; exit 1; }
  echo "preflight: ok"
}

bump_patch() {
  local cur next major minor patch f
  cur="$(sed -nE 's/^version = "([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' "$CAT/crates/catalyrst-types/Cargo.toml" | head -1)"
  [ -n "$cur" ] || { echo "bump: could not read current version" >&2; exit 1; }
  IFS=. read -r major minor patch <<<"$cur"
  next="$major.$minor.$((patch + 1))"
  for f in "$CAT"/crates/*/Cargo.toml; do
    sed -i -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$next\"/" "$f"
  done
  run_cargo update --workspace >/dev/null 2>&1
  git -C "$CAT" commit -q -F - -- ':(glob)crates/**/Cargo.toml' Cargo.lock <<MSG
catalyrst: bump version to $next

Automated patch bump on export.
MSG
  echo "bump: catalyrst $cur -> $next"
}

if [ "$PUSH" = 1 ] && [ "$BUMP" = 1 ]; then
  bump_patch
fi

# AFTER the bump, so the gate runs on the exact tree that ships (a bump
# rewrites every crate version plus Cargo.lock; gating the pre-bump tree
# certified something the export does not contain).
nix_preflight

MANIFEST=(
  crates docs nixos contracts third_party
  sites ui3
  ':(exclude)docs/testing'
  ':(exclude)docs/bvimposters.md'
  ':(exclude)docs/futures-2026-07.md'
  # build-and-test.md documents the private one-way export process itself
  # (names export-catalyrst.sh + the SKIP_NIX_PREFLIGHT bypass) — internal, not product.
  ':(exclude)docs/build-and-test.md'
  # Any evidence/ directory anywhere under the shipped tree is local proof
  # material, not product — exclude by convention so a future evidence/ under
  # sites/, ui3/, or a crate is dropped without editing this list again.
  ':(exclude,glob)**/evidence/**'
  # ui3/src/generated/INDEX.json is a dev-only artefact index for the ts-rs
  # gate that cannot run standalone (it hardcodes the private monorepo layout
  # and a bevy-explorer sibling); its path fields also carry catalyrst/-prefixed
  # and bevy-explorer/crates/bridge_protocol references. No external value —
  # drop it rather than ship dangling private-sibling paths.
  ':(exclude)ui3/src/generated/INDEX.json'
  scripts/schemathesis
  .github .gitignore clippy.toml .cargo/audit.toml
  flake.nix flake.lock rust-toolchain.toml Cargo.toml Cargo.lock
  README.md DEPLOYMENT.md LICENSE seed-third-party.sql
)

if [ -d "$TARGET/.git" ] && [ -n "$(git -C "$TARGET" status --porcelain)" ]; then
  echo "REFUSING: $TARGET has uncommitted changes (one-way export would clobber them)." >&2
  git -C "$TARGET" status --short >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/tree"

( cd "$CAT" && git archive HEAD -- "${MANIFEST[@]}" ) | tar -x -C "$TMP/tree"
[ -d "$TMP/tree/crates" ] || { echo "manifest matched no tracked files?" >&2; exit 1; }

PI="$TMP/tree/crates/catalyrst-credits/src/purchase_intent.rs"
if [ -f "$PI" ]; then
  sed -i 's/dcl\.one Checkout/Catalyst Checkout/g' "$PI"
fi
CO="$TMP/tree/crates/catalyrst-server/src/cors.rs"
if [ -f "$CO" ]; then
  sed -i 's|https://dcl\.one|https://catalyst.example.com|g' "$CO"
fi
NC="$TMP/tree/crates/catalyrst-notifications/src/config.rs"
if [ -f "$NC" ]; then
  sed -i 's|https://dcl\.one/marketplace/|https://marketplace.example.com/|g' "$NC"
fi
# [package.metadata.generated] paths are git-root-relative in the private tree
# (sites/ and ui3/ live under catalyrst/ there). The export root IS catalyrst,
# so drop the prefix to keep the contract meaningful against this layout.
find "$TMP/tree/crates" -name Cargo.toml -exec sed -i -E \
  's#^(\s*(gate|ts-bindings|openapi) *= *")catalyrst/#\1#' {} +
# Same git-root-relative -> export-root-relative fixup for the ts-rs gate's
# shared config: strip the catalyrst/ prefix from the paths it derives, and
# walk one fewer level up from sites/scripts (sites/ sits directly under the
# export root here, not under catalyrst/). The bridge_protocol reference is
# left alone: it lives in the public decentraland/bevy-explorer repo, not in
# this export, and that half of the gate is inherently cross-repo.
TSC="$TMP/tree/sites/scripts/ts-crates.sh"
if [ -f "$TSC" ]; then
  sed -i \
    -e 's#/\.\./\.\./\.\.#/../..#' \
    -e 's#TS_CRATES_ROOT/catalyrst/#TS_CRATES_ROOT/#g' \
    -e 's|TS_CRATES_ROOT/catalyrst#ci|TS_CRATES_ROOT#ci|g' \
    -e 's#GENERATED_DIR_REL="catalyrst/#GENERATED_DIR_REL="#' \
    -e 's#catalyrst/sites/#sites/#g' \
    "$TSC"
fi

FW="$TMP/tree/crates/catalyrst-notifications/src/first_wear.rs"
if [ -f "$FW" ]; then
  sed -i 's|https://catalyst\.dcl\.one|https://catalyst.example.com|g' "$FW"
fi
BVD="$TMP/tree/crates/catalyrst-bvimposters"
if [ -d "$BVD" ]; then
  find "$BVD" -type f \( -name '*.rs' -o -name '*.md' \) -exec sed -i \
    -e 's|https://catalyst\.dcl\.one|https://catalyst.example.com|g' \
    -e 's|/home/dcl/one/umbrella/data/bvimposters|/var/lib/bvimposters|g' \
    -e 's|/home/dcl/one/bevy-explorer/target/debug/impost|impost|g' \
    -e 's|Binding contract: `/home/dcl/one/umbrella/docs/bvimposters\.md`\. ||g' \
    {} +
fi
WF="$TMP/tree/crates/catalyrst-worlds"
if [ -d "$WF" ]; then
  find "$WF" -type f \( -name '*.rs' -o -name '*.example' \) -exec sed -i \
    -e 's|umbrella/config/federation-peers|deploy/config/federation-peers|g' \
    -e 's|catalyst\.dcl\.one|catalyst.example.com|g' \
    {} +
fi
# The explorer-api SSRF-guard comment names the literal cloud-metadata IMDS
# address (169.254.169.254, a link-local); the leak gate forbids 169.254. — the
# guard logic itself uses is_link_local(), so only the doc comment carries it.
RP="$TMP/tree/crates/catalyrst-explorer-api/src/modules/realm_provider.rs"
if [ -f "$RP" ]; then
  sed -i 's|169\.254\.169\.254|the cloud-metadata IMDS|g' "$RP"
fi
# Rewritten rather than fixed at the source: sqlx checksums applied migrations,
# so the in-tree bytes must not change.
PM="$TMP/tree/crates/catalyrst-places/migrations/0002_place_indexed.sql"
if [ -f "$PM" ]; then
  sed -i \
    -e 's|(umbrella/scripts/sync-archive-copies\.sh) ||' \
    -e "s|Refilled by umbrella/scripts/sync-world-places\.sh\.|Refilled by the deployment's own world-places sync.|" \
    "$PM"
fi
P4="$TMP/tree/crates/catalyrst-places/migrations/0004_place_plain_text.sql"
if [ -f "$P4" ]; then
  sed -i "s|umbrella/scripts/sync-world-places\.sh copies|the deployment's world-places sync copies|" "$P4"
fi
LA="$TMP/tree/crates/catalyrst-land-authz/migrations/0001_land_authz.sql"
if [ -f "$LA" ]; then
  sed -i "s|umbrella/scripts/bootstrap-land-authz\.sh|the deployment's land-authz bootstrap|" "$LA"
fi
CB="$TMP/tree/docs/crate-boundaries.md"
if [ -f "$CB" ]; then
  sed -i 's/export-catalyrst\.sh/the export tooling/g' "$CB"
fi
CF="$TMP/tree/crates/catalyrst-conformance/fixtures"
if [ -d "$CF" ]; then
  find "$CF" -name '*.json' -exec sed -i 's|https://dcl\.one|https://catalyst.example.com|g' {} +
fi
WEB=()
for d in sites ui3; do
  [ -d "$TMP/tree/$d" ] && WEB+=("$TMP/tree/$d")
done
if [ "${#WEB[@]}" -gt 0 ]; then
  grep -rIlZE 'dcl\.one|\bumbrella\b' "${WEB[@]}" | xargs -0 -r sed -i \
    -e 's|catalyst\.dcl\.one|catalyst.example.com|g' \
    -e 's|creators-data\.dcl\.one|creators-data.example.com|g' \
    -e 's|livekit\.dcl\.one|livekit.example.com|g' \
    -e 's|sites\.dcl\.one|sites.example.com|g' \
    -e 's|telemetry\.dcl\.one|telemetry.example.com|g' \
    -e 's|worlds\.dcl\.one|worlds.example.com|g' \
    -e 's|dcl\.one|catalyst.example.com|g' \
    -e 's|, umbrella/ or| or|g' \
    -e 's|umbrella/env/|deploy/env/|g' \
    -e 's|umbrella/metabase/|deploy/metabase/|g' \
    -e 's/\bumbrella-/deploy-/g'
fi
SF="$TMP/tree/sites/flake.nix"
if [ -f "$SF" ]; then
  sed -i 's/\bumbrella\b/the deployment/g' "$SF"
fi
# Personal-email hygiene: no @gmail.com address ships (creator-contact fixture
# fields, a design-doc owner line, an experiment-assignment test sid). The
# sanitize gate fails closed on any @gmail.com; this pass redacts the known
# ones so that gate stays a safety net rather than a blocker.
grep -rIlZaE '@[Gg][Mm][Aa][Ii][Ll]\.[Cc][Oo][Mm]' "$TMP/tree" | xargs -0 -r sed -i -E \
  's/[A-Za-z0-9._%+-]+@[Gg][Mm][Aa][Ii][Ll]\.[Cc][Oo][Mm]/redacted@example.com/g'
cat >> "$TMP/tree/README.md" <<'EOF'

## Related repositories

The standalone asset-bundle converter + AB-parity compare pipeline is
maintained separately at [decentraland/abgen](https://github.com/decentraland/abgen).
EOF

if command -v cargo >/dev/null 2>&1; then
  ( cd "$TMP/tree" && cargo fmt --all )
else
  ( cd "$TMP/tree" && nix develop "$CAT" --command cargo fmt --all )
fi

bash "$HERE/export-sanitize-gate.sh" "$TMP/tree"

if [ ! -d "$TARGET/.git" ]; then
  mkdir -p "$TARGET"
  git -C "$TARGET" init -q -b main
  git -C "$TARGET" config user.name  "Esteban Ordano"
  git -C "$TARGET" config user.email "esteban@decentraland.org"
  echo "export: initialized fresh git repo at $TARGET"
fi
git -C "$TARGET" remote get-url origin >/dev/null 2>&1 \
  || git -C "$TARGET" remote add origin git@github.com:eordano/catalyrst.git

push_target() {
  [ "$PUSH" = 1 ] || return 0
  local attempt
  for attempt in 1 2 3 4 5; do
    if SSH_AUTH_SOCK= \
       GIT_SSH_COMMAND="ssh -F /dev/null -i ${GH_EXPORT_KEY:-/home/dcl/one/credentials/gh-eordano} -o IdentitiesOnly=yes -o ConnectTimeout=10" \
       git -C "$TARGET" push -f origin main; then
      return 0
    fi
    echo "export: push attempt $attempt failed — retrying in 5s" >&2
    sleep 5
  done
  echo "export: push failed after 5 attempts" >&2
  exit 1
}

rsync -rlpt --delete --exclude=.git "$TMP/tree"/ "$TARGET"/

if [ -z "$(git -C "$TARGET" status --porcelain)" ]; then
  SRC_SHA="$(git -C "$CAT" rev-parse --short HEAD)"
  if git -C "$TARGET" rev-parse HEAD >/dev/null 2>&1 \
     && ! git -C "$TARGET" log -1 --format=%B | grep -q "@$SRC_SHA"; then
    git -C "$TARGET" commit -q --amend -m "catalyrst: Rust Decentraland catalyst (content + lambdas + services)

Generated one-way export from the catalyrst workspace @$SRC_SHA."
    echo "export: no content diff — restamped provenance @$SRC_SHA"
  else
    echo "export: no diff — $TARGET already converged"
  fi
  push_target
  exit 0
fi

SHA="$(git -C "$CAT" rev-parse --short HEAD)"
git -C "$TARGET" add -A
MSG="catalyrst: Rust Decentraland catalyst (content + lambdas + services)

Generated one-way export from the catalyrst workspace @$SHA."
if git -C "$TARGET" rev-parse HEAD >/dev/null 2>&1; then
  git -C "$TARGET" commit -q --amend -m "$MSG"
else
  git -C "$TARGET" commit -q -m "$MSG"
fi
echo "export: amended single export commit @$SHA in $TARGET"
push_target
