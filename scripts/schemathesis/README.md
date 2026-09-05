# schemathesis contract fuzzer

Wrapper around [schemathesis](https://schemathesis.readthedocs.io/): property-based fuzzing of a live
API against the OpenAPI spec at `docs/openapi.yaml` (generated separately). Finds undocumented 5xx,
spec-nonconforming responses, missing CORS headers, and error bodies violating the catalyrst error contract.

Quick start against a local catalyrst (first run creates `scripts/schemathesis/.venv` and installs
schemathesis into it; later runs reuse it; no system pip is touched):

```sh
./scripts/schemathesis/run.sh --target http://127.0.0.1:5141
```

| flag                          | default              | notes                                  |
|-------------------------------|----------------------|----------------------------------------|
| `--target <url>`              | (required)           | Base URL of the running API            |
| `--spec <path>`               | `docs/openapi.yaml`  | Path to OpenAPI spec                   |
| `--checks <list>`             | `all`                | Comma-separated schemathesis checks    |
| `--hypothesis-max-examples N` | `50`                 | Examples generated per operation       |
| `--workers N`                 | `2`                  | Parallel workers                       |
| `--report <path>`             | (none)               | Optional JUnit XML output path         |

Any catalyst-compatible peer works as a target, but public peers rate-limit (HTTP 429) aggressively:
keep `--hypothesis-max-examples` low and `--workers 1`, and do not run on shared infrastructure unannounced.

```sh
./scripts/schemathesis/run.sh --target https://peer.decentraland.org \
    --hypothesis-max-examples 5 --workers 1
```

CI invocation (the wrapper exits with schemathesis' exit code, so CI can fail on it directly):

```sh
./scripts/schemathesis/run.sh --target http://127.0.0.1:5141 --hypothesis-max-examples 25 --workers 4 --report target/schemathesis-junit.xml
```

## Custom checks (`checks.py`, auto-loaded via `SCHEMATHESIS_HOOKS=checks`)

- `not_a_server_error` - any 5xx not listed (or `default`) in the operation's `responses` block fails.
- `response_schema_conformance` - re-export of the built-in check so the enforced set is enumerated in one file.
- `cors_headers_present` - a request carrying `Origin` must get back `Access-Control-Allow-Origin` (the origin or `*`).
- `error_body_shape` - non-2xx JSON must be an object with a string `error` field and an optional
  string `message` field, per the catalyrst error contract.

Known false-positive territory: if `docs/openapi.yaml` lags actual behavior, expect
`response_schema_conformance` noise on changed endpoints - regenerate the spec (workflow in the
top-level `docs/` directory) before chasing failures.

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
