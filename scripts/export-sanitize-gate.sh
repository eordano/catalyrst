#!/usr/bin/env bash
set -euo pipefail

TREE="${1:?usage: $0 <tree>}"

EXTRA_EX=()
if [ -n "${GATE_EXTRA_EXCLUDE_DIRS:-}" ]; then
  IFS=',' read -ra _xs <<<"$GATE_EXTRA_EXCLUDE_DIRS"
  for _x in "${_xs[@]}"; do EXTRA_EX+=("--exclude-dir=$_x"); done
fi

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
  'credentials/'
  'BEGIN[ A-Z]*PRIVATE KEY'
  'dcl\.one'
  # '\bcatalyrst\b' is not forbidden here: both callers of THIS gate scan
  # catalyrst-named trees where the name is public.
  'export-(catalyrst|sanitize-gate|automation-bridge)\.sh'
  'one-way export'
  'Claude-Session:'
  'dcl\.social'
  '169\.254\.'
  '\bumbrella\b'
  '\bmars\b'
  '\bforgejo\b'
  '\blorebook\b'
  'dcl\.tools'
  # Personal email — fail closed. The general gmail form plus the specific
  # maintainer literal (belt and suspenders). The export sanitizes any
  # @gmail.com to example.com before this runs, so a hit here means the
  # rewrite missed one — abort rather than ship it.
  '[A-Za-z0-9._%+-]+@[Gg][Mm][Aa][Ii][Ll]\.[Cc][Oo][Mm]'
  'eordano@[Gg][Mm][Aa][Ii][Ll]'
  # RFC1918 private ranges (10/8 and 172.16/12) as full dotted quads — a
  # leaked dev-infra address fails closed. The well-known-benign SDK/test/doc
  # example addresses are exempted by exact path:line in ALLOW_LOC below, so a
  # real address at any OTHER location still fails.
  '\b10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b'
  '\b172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}\b'
)

# Two exemption tiers, both narrow enough that a real secret which merely
# SHARES A SUBSTRING with an entry still fails (the old gate substring-matched
# the whole hit against a flat token list, so a genuine leak sharing a token
# with a benign example slipped through):
#
#   ALLOW_LOC    — exact tree-relative "path:line" of each verified-benign hit
#                  (RFC1918 / link-local / CGNAT example IPs in the LAN-detection
#                  SDK, the SSRF sanitiser tests, is_loopback_host asserts, the
#                  QR local-preview URLs). Matched WHOLE-LINE against the hit's
#                  own "path:line", so the same value leaking from any other
#                  location — or a real secret landing where one of these used
#                  to sit after the line shifts — is NOT exempted. Regenerate
#                  after moving these lines:
#                    grep -rInaE -e '169\.254\.' -e '100\.64\.' -e '192\.168\.' \
#                      -e '\b10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b' \
#                      -e '\b172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}\b' \
#                      "$TREE" | sed -E "s#^$TREE/##;s#^([^:]+):([0-9]+):.*#\1:\2#" | sort -u
#
#   ALLOW_PHRASE — a handful of fixed strings benign wherever they appear (OS
#                  user-path placeholders, an emoji shortcode, a wearable colour
#                  name, the base64 body of a TEST key). None can plausibly be a
#                  substring of a real secret, so substring match is safe here;
#                  IP-range values are deliberately kept OUT of this tier.
ALLOW_LOC="$(mktemp)"
ALLOW_PHRASE="$(mktemp)"
trap 'rm -f "$ALLOW_LOC" "$ALLOW_PHRASE"' EXIT
cat >"$ALLOW_LOC" <<'LOC'
crates/catalyrst-credits/src/config.rs:284
crates/catalyrst-economy/src/config.rs:255
crates/catalyrst-envcfg/src/lib.rs:192
crates/catalyrst-envcfg/src/lib.rs:195
crates/catalyrst-events/src/ports/events.rs:1019
crates/catalyrst-events/src/sanitize.rs:306
crates/catalyrst-events/src/sanitize.rs:324
crates/catalyrst-events/src/sanitize.rs:489
crates/catalyrst-events/src/sanitize.rs:490
crates/catalyrst-events/src/sanitize.rs:491
crates/catalyrst-events/src/sanitize.rs:492
crates/catalyrst-events/src/sanitize.rs:493
crates/catalyrst-events/src/sanitize.rs:494
crates/catalyrst-market/src/config.rs:137
crates/catalyrst-places/src/sanitize.rs:329
crates/catalyrst-places/src/sanitize.rs:343
crates/catalyrst-pulse/src/server/tests.rs:755
crates/catalyrst-scene-state/src/jsruntime/fetch.rs:293
crates/catalyrst-scene-state/src/jsruntime/fetch.rs:297
crates/catalyrst-scene-state/src/jsruntime/fetch.rs:319
crates/catalyrst-worlds/src/fed/peers.rs:848
crates/catalyrst-worlds/tests/federation_peer_admission.rs:729
crates/catalyrst-worlds/tests/federation_peer_admission.rs:730
crates/dcl-one-sdk/README.md:101
crates/dcl-one-sdk/src/joinblock.rs:233
crates/dcl-one-sdk/src/joinblock.rs:403
crates/dcl-one-sdk/src/joinblock.rs:405
crates/dcl-one-sdk/src/joinblock.rs:406
crates/dcl-one-sdk/src/joinblock.rs:418
crates/dcl-one-sdk/src/joinblock.rs:433
crates/dcl-one-sdk/src/joinblock.rs:436
crates/dcl-one-sdk/src/joinblock.rs:443
crates/dcl-one-sdk/src/joinblock.rs:468
crates/dcl-one-sdk/src/joinblock.rs:469
crates/dcl-one-sdk/src/joinblock.rs:471
crates/dcl-one-sdk/src/joinblock.rs:507
crates/dcl-one-sdk/src/joinblock.rs:510
crates/dcl-one-sdk/src/joinblock.rs:527
crates/dcl-one-sdk/src/joinblock.rs:536
crates/dcl-one-sdk/src/joinblock.rs:554
crates/dcl-one-sdk/src/joinblock.rs:558
crates/dcl-one-sdk/src/joinblock.rs:568
crates/dcl-one-sdk/src/joinblock.rs:570
crates/dcl-one-sdk/src/joinblock.rs:572
crates/dcl-one-sdk/src/joinblock.rs:599
crates/dcl-one-sdk/src/netinfo.rs:89
crates/dcl-one-sdk/src/netinfo.rs:90
crates/dcl-one-sdk/src/netinfo.rs:91
crates/dcl-one-sdk/src/netinfo.rs:93
crates/dcl-one-sdk/src/netinfo.rs:97
crates/dcl-one-sdk/src/netinfo.rs:98
crates/dcl-one-sdk/src/netinfo.rs:99
crates/dcl-one-sdk/src/netinfo.rs:100
crates/dcl-one-sdk/src/netinfo.rs:101
crates/dcl-one-sdk/src/netinfo.rs:106
crates/dcl-one-sdk/src/netinfo.rs:108
crates/dcl-one-sdk/src/netinfo.rs:111
crates/dcl-one-sdk/src/netinfo.rs:117
crates/dcl-one-sdk/src/netinfo.rs:119
crates/dcl-one-sdk/src/netinfo.rs:125
crates/dcl-one-sdk/src/netinfo.rs:126
crates/dcl-one-sdk/src/start/mod.rs:748
crates/dcl-one-sdk/src/start/mod.rs:754
crates/dcl-one-sdk/src/start/mod.rs:758
crates/dcl-one-sdk/docs/upstream/sdk-skills/PR.md:273
sites/packages/data/src/lib/catalyst/client.test.ts:73
ui3/src/creatorhub/components/ChModalMobileQRCode.stories.tsx:5
ui3/src/creatorhub/components/ChModalMobileQRCode.tsx:83
LOC
cat >"$ALLOW_PHRASE" <<'PHRASE'
/Users/YOU
/Users/USER
C:/Users/$
eyebrow shade
eyebrow.shade
/Users/UserName/Library/Logs
:umbrella:
PRIVATE KEY-----\naGVsbG8=
PHRASE

fail=0
for p in "${PATTERNS[@]}"; do
  hits="$(grep -rInaE --exclude-dir=.git --exclude-dir=Library --exclude-dir=Temp --exclude-dir=Logs --exclude-dir=obj ${EXTRA_EX[@]+"${EXTRA_EX[@]}"} -e "$p" "$TREE" 2>/dev/null \
    | awk -v tree="$TREE/" -v locf="$ALLOW_LOC" -v phrf="$ALLOW_PHRASE" '
        BEGIN {
          while ((getline l < locf) > 0) if (l != "" && l !~ /^#/) loc[l] = 1
          np = 0
          while ((getline l < phrf) > 0) if (l != "" && l !~ /^#/) ph[++np] = l
        }
        {
          line = $0
          if (index(line, tree) == 1) line = substr(line, length(tree) + 1)
          # derive the "relpath:line" key by dropping the ":content" tail
          c1 = index(line, ":")
          rest = substr(line, c1 + 1)
          c2 = index(rest, ":")
          key = (c1 > 0 && c2 > 0) ? substr(line, 1, c1 + c2 - 1) : line
          if (key in loc) next
          for (i = 1; i <= np; i++) if (index(line, ph[i])) next
          print line
        }' \
    | head -40 || true)"
  if [ -n "$hits" ]; then
    echo "FORBIDDEN pattern '$p' in generated tree:" >&2
    printf '%s\n' "$hits" >&2
    fail=1
  fi
done

if [ "$fail" != 0 ]; then
  echo "sanitation gate FAILED — export aborted (fix the source in catalyrst," >&2
  echo "add a per-target rewrite, or exclude the file from the manifest)" >&2
  exit 1
fi
echo "sanitation gate: clean ($TREE)"
