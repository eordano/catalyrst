# Conformance fixtures

Reference (request, response) pairs captured against a known-good catalyst
peer (typically `https://peer.decentraland.org`). `catalyrst-conformance-replay`
loads every `*.json` under this tree, re-issues each request against a
candidate host, and diffs the response - CI parity without a live network.

## Capture

```sh
# GET request - body omitted
cargo run -p catalyrst-conformance --bin catalyrst-conformance-capture -- \
  --peer https://peer.decentraland.org \
  --output crates/catalyrst-conformance/fixtures/edge-cases/content/about-golden.json \
  GET /content/about

# POST request with a JSON body (quote it as ONE arg)
cargo run -p catalyrst-conformance --bin catalyrst-conformance-capture -- \
  --peer https://peer.decentraland.org \
  --output crates/catalyrst-conformance/fixtures/edge-cases/content/active-entities-pointer-00.json \
  POST /content/entities/active '{"pointers": ["0,0"]}'

# Mark per-fixture fields that may legitimately differ on replay
cargo run -p catalyrst-conformance --bin catalyrst-conformance-capture -- \
  --peer https://peer.decentraland.org \
  --output crates/catalyrst-conformance/fixtures/edge-cases/content/status.json \
  --volatile-paths 'currentTime,configurations.realmName' \
  GET /content/status
```

Multipart / form-data captures are not supported - `capture` errors out.

## Replay

```sh
# Replay everything under fixtures/
cargo run -p catalyrst-conformance --bin catalyrst-conformance-replay -- \
  --candidate http://127.0.0.1:5140 \
  --fixtures crates/catalyrst-conformance/fixtures/

# Filter by glob (relative to --fixtures)
cargo run -p catalyrst-conformance --bin catalyrst-conformance-replay -- \
  --candidate http://127.0.0.1:5140 \
  --fixtures crates/catalyrst-conformance/fixtures/ \
  --filter 'content/*'
```

Replay exits non-zero on any failure.

## Fixture JSON schema

```json
{
  "description": "Free-text human-readable label.",
  "request": {
    "method": "GET",
    "path": "/content/about",
    "query": {"key": "val"},
    "headers": {"accept": "application/json"},
    "body": null
  },
  "response": {
    "status": 200,
    "headers": {"content-type": "application/json"},
    "body_json": { "...": "parsed JSON body" }
  },
  "captured_from": "https://peer.decentraland.org",
  "captured_at": "2026-05-23T20:00:00Z",
  "volatile_paths": ["configurations.realmName", "content.publicUrl"]
}
```

- `request.body` - `serde_json::Value` or `null`; JSON bodies only.
- `response.body_json` vs `response.body_bytes_b64` - exactly one populated:
  a `content-type: application/json` response parses into `body_json` (diff
  ignores whitespace/key order); anything else stores base64 in
  `body_bytes_b64`, compared byte-for-byte.
- `response.headers` - capture records only `content-type`,
  `content-length`, `etag`, `cache-control`; replay asserts only
  `content-type` (ignoring any `; charset=...` suffix) and `content-length`.
- `captured_at` - RFC3339 UTC, second precision.
- `volatile_paths` - per-fixture allowlist of dot-separated response paths
  scrubbed to the same sentinel on both sides before diffing; adds to the
  global `IGNORED_FIELDS` in `diff.rs` (`currentTime`, `realmName`, ...).
  No array indexing - a component inside an array recurses into every
  element (`snapshots.url` scrubs `snapshots[*].url`).

JSON has no comments - annotate via `description`.

## Layout

```
fixtures/
  edge-cases/
    content/          # /content/* endpoints
    lambdas/          # /lambdas/* endpoints
    cors/             # CORS preflight / no-origin cases
    fallback/         # unknown-path 404 cases
  example/            # template fixtures (shipped with the crate)
```

One request per file; short descriptive names: `about-golden.json`,
`active-entities-pointer-00.json`, `profiles-by-address-vitalik.json`.
