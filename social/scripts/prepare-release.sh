#!/usr/bin/env bash
# Builds a reviewable release directory. Does not install, reload nginx or change DNS.
set -euo pipefail
social_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
workspace_dir="$(cd -- "$social_dir/.." && pwd)"
release_dir="${1:-/tmp/dcl-social-release-$(date -u +%Y%m%dT%H%M%SZ)}"
if [[ -e "$release_dir" ]]; then
  echo "Release destination already exists: $release_dir" >&2
  exit 1
fi
(cd "$social_dir" && npm ci --ignore-scripts && npm run build)
(cd "$workspace_dir" && cargo build --locked --release -p dcl-social-api)
target_dir="$(cd "$workspace_dir" && cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
mkdir -p "$release_dir/bin" "$release_dir/public" "$release_dir/deploy"
cp "$target_dir/release/dcl-social-api" "$release_dir/bin/"
cp -R "$social_dir/dist/." "$release_dir/public/"
cp "$social_dir/deploy/"* "$release_dir/deploy/"
{
  echo "dcl.social release"
  date -u +"Built: %Y-%m-%dT%H:%M:%SZ"
  git -C "$workspace_dir" rev-parse HEAD
  git -C "$workspace_dir" status --short -- social crates/dcl-social-api Cargo.toml Cargo.lock
} > "$release_dir/BUILD.txt"
ldd "$release_dir/bin/dcl-social-api" > "$release_dir/runtime-libraries.txt"
(cd "$release_dir" && find bin public deploy -type f -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
echo "Prepared $release_dir"
