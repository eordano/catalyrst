#!/usr/bin/env bash
# Design export: capture the preview server's pages as standalone HTML.
#
# Every page inlines its CSS and JS already; this script fetches each state,
# rewrites image srcs to data URLs, and copies the editable CSS/JS sources
# alongside. Point it at a running preview server.
#
#   scripts/export-ui.sh <output-dir> [server-url]
#
# The publish-flow states (inline wallet panel, published) exist only while a
# publish is in flight, so they are captured when present and skipped quietly
# when not.
set -euo pipefail

out="${1:?usage: export-ui.sh <output-dir> [server-url]}"
base="${2:-http://127.0.0.1:8000}"
src="$(cd "$(dirname "$0")/.." && pwd)/src"

mkdir -p "$out/sources"

fetch() {
  curl -sf -H 'Accept: text/html' "$base$1" -o "$out/$2" || {
    echo "skip $2 ($1 not reachable)"
    return 1
  }
  echo "saved $2"
}

fetch '/' index.html
fetch '/?where=web' index-web.html
fetch '/?where=phone' index-phone.html
fetch '/deploy' deploy.html
fetch '/deploy/sign/' sign-page.html || true

python3 - "$out" "$base" <<'PYEOF'
import base64, os, re, sys, urllib.request
out, base = sys.argv[1], sys.argv[2]
cache = {}
for name in os.listdir(out):
    if not name.endswith('.html'):
        continue
    path = os.path.join(out, name)
    html = open(path).read()
    def inline(m):
        url = m.group(1)
        if url not in cache:
            data = urllib.request.urlopen(base + url).read()
            cache[url] = 'data:image/png;base64,' + base64.b64encode(data).decode()
        return 'src="' + cache[url] + '"'
    rewritten = re.sub(r'src="(/[^"]+)"', inline, html)
    if rewritten != html:
        open(path, 'w').write(rewritten)
        print(f'inlined images in {name}')
PYEOF

for f in start/landing.css start/page_common.js start/landing_edit.js \
  start/deploy_page.js start/sign_flow.js; do
  cp "$src/$f" "$out/sources/"
done
echo "sources copied; export at $out"
