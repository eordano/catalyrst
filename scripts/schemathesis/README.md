# schemathesis contract fuzzer

This directory hosts a thin wrapper around [schemathesis](https://schemathesis.readthedocs.io/),
a property-based fuzzer that reads an OpenAPI spec and bombards a live API
with generated requests. It is used to find:

- Undocumented 5xx responses.
- Responses whose body/status do not match the spec.
- Missing CORS headers on cross-origin requests.
- Error bodies that do not conform to the catalyrst error contract.

The spec we drive it with lives at `docs/openapi.yaml` (generated separately).

## Quick start (local catalyrst)

```sh
# 1. Start catalyrst somewhere (e.g. cargo run, default port 5141).
# 2. From the repo root:
./scripts/schemathesis/run.sh --target http://127.0.0.1:5141
```

First run will create a venv at `scripts/schemathesis/.venv` and install
schemathesis into it. Subsequent runs reuse the venv. No system pip is touched.

## Arguments

| flag                          | default              | notes                                  |
|-------------------------------|----------------------|----------------------------------------|
| `--target <url>`              | (required)           | Base URL of the running API            |
| `--spec <path>`               | `docs/openapi.yaml`  | Path to OpenAPI spec                   |
| `--checks <list>`             | `all`                | Comma-separated schemathesis checks    |
| `--hypothesis-max-examples N` | `50`                 | Examples generated per operation       |
| `--workers N`                 | `2`                  | Parallel workers                       |
| `--report <path>`             | (none)               | Optional JUnit XML output path         |

## Running against a public peer

You can point this at any catalyst-compatible peer:

```sh
./scripts/schemathesis/run.sh --target https://peer.decentraland.org \
    --hypothesis-max-examples 5 --workers 1
```

WARNING: public peers will rate-limit (HTTP 429) aggressively. Keep
`--hypothesis-max-examples` low and `--workers 1`. Do not run this on
shared infrastructure unannounced.

## CI invocation

```sh
./scripts/schemathesis/run.sh --target http://127.0.0.1:5141 --hypothesis-max-examples 25 --workers 4 --report target/schemathesis-junit.xml
```

The wrapper exits with schemathesis' exit code, so CI can fail on it directly.

## Custom checks

Defined in `checks.py` and auto-loaded via `SCHEMATHESIS_HOOKS=checks`:

- **`not_a_server_error`** - any 5xx not listed (or `default`) in the
  operation's `responses` block is a failure.
- **`response_schema_conformance`** - re-export of schemathesis' built-in
  check so the enforced set is enumerated in one file.
- **`cors_headers_present`** - when the request carries an `Origin` header,
  the response must echo `Access-Control-Allow-Origin` (matching the origin
  or `*`).
- **`error_body_shape`** - any non-2xx JSON response must be an object with
  a string `error` field and an optional string `message` field, per the
  catalyrst error contract.

## Known false-positive territory: spec drift

If `docs/openapi.yaml` lags behind actual catalyrst behavior, you will see
noise - generally `response_schema_conformance` failures on endpoints whose
shape changed without a spec update. Regenerate the spec from the source of
truth before chasing those down (see the spec regeneration workflow in the
top-level `docs/` directory).

## Layout

```
scripts/schemathesis/
  README.md          this file
  run.sh             bash entrypoint, venv bootstrap + invoker
  requirements.txt   pinned schemathesis + hypothesis
  checks.py          custom @schemathesis.check functions
  conftest.py        minimal pytest scaffold for future fixtures
  .venv/             created on first run, gitignore'd
```
