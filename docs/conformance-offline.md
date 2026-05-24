# Offline conformance: capture once, replay forever

When the baseline is a rate-limited public peer (e.g. `peer.decentraland.org`),
running the full live-diff suite repeatedly is rude — it hits 429s and burns
operator goodwill. Instead, capture the baseline responses **once** to a
fixtures directory, then replay them against any candidate (local catalyrst,
a staging instance, a different peer) as many times as you want without ever
touching the baseline again.

## Two-step workflow

```bash
# 1. Capture (one-time, against the public peer).
#    --capture-to switches the runner into single-peer mode:
#    only the baseline is contacted; the candidate URL is ignored.
#    Use --inter-request-delay-ms to avoid getting throttled.
./target/debug/catalyrst-conformance \
  --baseline https://peer.decentraland.org \
  --candidate http://localhost:5141 \
  --capture-to ./oracle-fixtures \
  --inter-request-delay-ms 1500

# 2. Replay (repeatable, offline — no baseline contact).
./target/debug/catalyrst-conformance-replay \
  --candidate http://localhost:5141 \
  --fixtures ./oracle-fixtures
```

After step 1, `./oracle-fixtures/` contains one `.json` per captured case
(content endpoints under `content/`, lambdas under `lambdas/`). Filename
shape: `<slug>-<hash>.json` where the hash disambiguates POSTs with
different bodies under the same URL.

Step 2 can be run unlimited times. It hits only the candidate.

## Rate-limit considerations during capture

`peer.decentraland.org` throttles `/deployments` aggressively. Recommended
delays for a clean capture:

| Peer                              | `--inter-request-delay-ms` |
|-----------------------------------|----------------------------|
| Local catalyst (no rate limit)    | `0` (default)              |
| `peer-eu1.decentraland.org`       | `500`                      |
| `peer.decentraland.org`           | `1500`                     |
| Lightly-loaded community peer     | `500-1000`                 |

If a case still drains its retry budget (3 attempts with 1s/2s/4s
back-off + `Retry-After` honoring), it's recorded as a transient skip
and NO fixture is written. Re-run capture later with a longer delay or
against a different peer.

## What capture mode does NOT touch

In capture mode, the candidate URL is never resolved or connected to.
You can pass any value (or omit `--candidate` to use the default
`http://127.0.0.1:5141`) — it's only used for the printed banner.

## What replay verifies

For each fixture:

- **Status code** must match.
- **Body:** JSON responses diff structurally via the same `compare_json`
  used by the live runner, honoring per-fixture `volatile_paths` (scrubbed
  to `<<VOLATILE>>` on both sides before diff). Non-JSON bodies are
  compared byte-for-byte.
- **Headers:** `content-type` (charset-suffix tolerant) and
  `content-length` only.

If you need a different volatility allowlist for a fixture, edit the
`volatile_paths` array in the fixture JSON by hand — replay reads it
directly.

## When live-diff still makes sense

- A peer you control (no rate limit) — live runs are immediate and don't
  produce stale fixtures.
- Drift detection: a nightly job that captures against the public peer
  and `diff`s the new fixtures against the committed ones tells you when
  upstream behavior changes.
- Iterating on catalyrst behavior: re-running capture is overkill if you
  haven't touched the handlers.

## Refresh workflow

```bash
# Refresh all fixtures from the public peer.
rm -rf oracle-fixtures && \
  ./target/debug/catalyrst-conformance \
    --baseline https://peer.decentraland.org \
    --capture-to oracle-fixtures \
    --inter-request-delay-ms 1500

# Diff against the previously-committed fixtures (git tells you what changed).
git status oracle-fixtures
git diff --stat oracle-fixtures
```

Commit the new fixtures only after eyeballing the diff — upstream might
have added a field you actually want to start asserting on.
