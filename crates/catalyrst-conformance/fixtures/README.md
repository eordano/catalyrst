# Conformance fixtures

This directory stores **golden** (request, response) pairs captured against a
known-good catalyst peer (typically `https://peer.decentraland.org`). The
`catalyrst-conformance-replay` binary loads every `*.json` file under this
tree, re-issues the recorded request against a candidate host, and diffs the
response against the recorded one.

Fixtures let CI assert parity without a live network dependency, and let
contributors reproduce a single failing case offline.

## How to capture

```sh
# GET request — body omitted
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

Multipart / form-data captures are **not** supported — `capture` errors out
clearly if you try.

## How to replay

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

Replay exits non-zero on any failure, so it drops straight into CI.

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

### Field notes

- `request.body` — `serde_json::Value` or `null`. We only support JSON request
  bodies. Multipart uploads are out of scope for capture/replay.
- `response.body_json` vs `response.body_bytes_b64` — **exactly one** is
  populated. If the response advertised `content-type: application/json`,
  capture parses the body into `body_json` so the diff machinery can ignore
  whitespace / key-order. Otherwise the raw bytes are stored as base64 under
  `body_bytes_b64` and replay does a byte-for-byte compare.
- `response.headers` — capture only records `content-type`, `content-length`,
  `etag`, and `cache-control`. Replay only *asserts* on `content-type` and
  `content-length`, and `content-type` comparison ignores any `; charset=...`
  suffix.
- `captured_at` — RFC3339 UTC, second precision.
- `volatile_paths` — optional per-fixture allowlist of dot-separated paths in
  the response that may differ on replay. Each path is scrubbed to the same
  sentinel on both sides before diffing. This is in addition to the global
  `IGNORED_FIELDS` list inside `diff.rs` (`currentTime`, `realmName`, ...).
  Array indexing is not supported in the path — if a path component lands
  inside an array, the scrubber recurses into every element. Example:
  `snapshots.url` will scrub `snapshots[*].url`.

JSON does not allow comments — if you need to annotate a fixture, put the
explanation in `description`.

## Layout convention

```
fixtures/
  edge-cases/
    content/          # /content/* endpoints
    lambdas/          # /lambdas/* endpoints
    cors/             # CORS preflight / no-origin cases
    fallback/         # unknown-path 404 cases
  example/            # template fixtures (this is what shipped with the crate)
```

Each fixture file is one request. Keep filenames short and descriptive:
`about-golden.json`, `active-entities-pointer-00.json`,
`profiles-by-address-vitalik.json`.

> _Re-verified against the real fixture tree 2026-07-03 (docs-stale-audit); corrections applied._
