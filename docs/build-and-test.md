# Building and testing

## Toolchain facts

- Stable Rust, no pinned toolchain file; per-shell pins: `nix develop` (default, nixpkgs stable ~1.95), `.#ci` (rust-overlay 1.97.0), `.#gpu` (adds CUDA/vulkan loader env). `protoc` required (dcl-rpc protobuf codegen in social-rpc/quests).
- HTTP stack is rustls, but `openssl-sys` arrives transitively via the Helios consensus light-client - system OpenSSL (+`pkg-config`) needed at compile time; `OPENSSL_NO_VENDOR=1` links it.
- The `nats` feature (federation live gossip) is off by default; build `-p catalyrst-fed --features nats` (and the embedding binary) for the transport.

The flake devShell (NixOS) carries the toolchain (cargo, rustc, rustfmt, clippy, rust-analyzer, protoc, cmake, openssl, turbojpeg) with `OPENSSL_NO_VENDOR`, `RUSTY_V8_ARCHIVE` (pinned librusty_v8), and `TURBOJPEG_LIB` preset:

```bash
nix develop            # or: nix develop -c cargo check --workspace
nix build .#catalyrst          # catalyrst-live (pinned, reproducible)
nix build .#catalyrst-all      # ~13-binary bundle package
```

Keep `CARGO_HOME` on persistent disk if /tmp is small/volatile. Nix builds compile only committed/tracked files - untracked files `cargo` uses vanish from `nix build`; track at creation.

Pin/patch rationale:

- Helios: all seven `helios-*` crates from one git revision - a single `outputHashes` entry.
- `librusty_v8` pinned via `crates/catalyrst-scene-state/nix/librusty_v8.nix` (scene-state embeds a JS runtime).
- `doCheck = false` across flake packages - tests run via cargo, not nix builds.

## Integration suites fail when their dependency is missing

A check that cannot distinguish "clean" from "did not run" is not a check. Every
suite that needs an external dependency (a Postgres, a NATS broker, a node
toolchain, a cargo feature) goes through `catalyrst-testgate`:

- Absent dependency: the test FAILS, with a message naming the variable.
- `ALLOW_SKIPPED_INTEGRATION=1`: it skips instead, printing a `SKIPPED` line
  and appending to `$CATALYRST_TESTGATE_SKIPLOG` when that is set. This is
  the developer-laptop escape hatch; CI must never set it.
- A dependency you explicitly configured but that doesn't work is always a
  hard failure - the opt-out does not cover it (`testgate::unusable`).

One variable runs every Postgres-backed suite in the workspace:

```bash
CATALYRST_TEST_PG=postgres://postgres@127.0.0.1:5432/postgres cargo test --workspace
```

The per-crate variables (`CATALYRST_ECONOMY_TEST_PG`, `CATALYRST_PLACES_TEST_PG`,
`CREDITS_TEST_PG_CONNECTION_STRING`, ...) still override it per crate. Each suite
creates and drops its own scratch schema or database, so pointing them all at one
server is safe.

Feature-gated suites carry an armed companion so they cannot vanish silently:
`cargo test -p catalyrst-social-service` fails until you pass `--features rpc`
(three `required-features` targets are otherwise omitted by cargo), and
`cargo test -p catalyrst-fed` fails until you pass `--features nats`.

New suites: add `catalyrst-testgate = { workspace = true }` to dev-dependencies
and reach for `require_pg` / `require_env` / `unavailable` / `unusable` rather
than `std::env::var(...).ok()?`. Suites built on
`catalyrst_contract_gate::pg::{ScratchSchema, ScratchDb}` inherit the gate for
free.

Two other languages carry the same contract on the same variable and the same
marker: `catalyrst/sites/test/e2e/require-dep.ts` and `rig/lib/testgate.sh`. What enforces
all three is `scripts/no-silent-skips.sh` at the repo root - it refuses to run
at all when the opt-out is set, then fails on any `^SKIPPED ` line. Its own bite
is proven by `scripts/no-silent-skips-selftest.sh`.

### The count gate

Arming the gates makes an unrun test fail, but it cannot catch a suite that
compiles to zero tests - a `required-features` target cargo omits, or a file
whose `#[test]`s were deleted. `scripts/test-count-gate.sh` is that backstop: it
runs every target listed in `tests-manifest.tsv` and fails when fewer than
`min_passed` tests pass, when the target produces no test binary, or when cargo
refuses to build it. It exits 2 if `ALLOW_SKIPPED_INTEGRATION` is set. Raise
`min_passed` in the manifest whenever you add tests to a listed target.

```bash
CATALYRST_TEST_PG=... scripts/test-count-gate.sh                  # every target
CATALYRST_TEST_PG=... scripts/test-count-gate.sh catalyrst-economy # one crate
scripts/test-count-gate.sh --list
```

## Test surfaces

| Harness | What it proves | How |
|---|---|---|
| unit tests | per-crate logic incl. parity canaries (snapshot progression vectors, boundary double-count, pointer-changes URL resolution, wire-shape regressions) | `cargo test --workspace` |
| `catalyrst-conformance` | live A/B parity of two hosts; bootstraps inputs from the baseline, diffs `/content`, `/lambdas` | `cargo run -p catalyrst-conformance -- --baseline <ref> --candidate <ours>` |
| `catalyrst-conformance-capture` / `-replay` | recorded-fixture parity, offline/CI-friendly; fixtures in `crates/catalyrst-conformance/fixtures/`; state-dependent fields masked by `volatility.toml`, per-fixture `volatile_paths` | capture once against a peer, replay forever |
| `catalyrst-oracle-tests` | foundation crates (hashing, crypto, validator, storage) reproduce vectors from a live catalyst DB - CIDs, auth chains, entity parses, on-disk sha1-prefix paths | `cargo run -p catalyrst-oracle-tests --bin extract` (needs `CATALYRST_ORACLE_DB_URL`, `CATALYRST_ORACLE_CONTENT_ROOT`), then `cargo test -p catalyrst-oracle-tests -- --ignored`; `test-vectors/` generated, not committed |
| `scripts/schemathesis/` | property-based fuzzing of a running server against [`docs/openapi.yaml`](./openapi.yaml); checks for 5xx, schema conformance, CORS, error-body shape | `scripts/schemathesis/run.sh --target http://127.0.0.1:5141` |
| `catalyrst-fuzz`, `catalyrst-bench` | fuzz/stress harnesses; criterion benches for hot paths (persisted previous results give delta-p50/p99 regression columns) | `cargo bench` etc. |
| federation gossip | in-process loop test (`tests/gossip_loop.rs`, broker-free); `nats_live` needs `--features nats` and a broker at `FED_NATS_URL`, and fails without either | see [federation.md](./federation.md) |
| abgen gates | fork-parity byte ratio, live-mode structural diff, render gates | upstream `decentraland/abgen` - see [architecture.md](./architecture.md) |

Three sources of truth, cheapest first: unit tests pin invariants; conformance/oracle pin wire, byte behavior against reference data; for client-facing questions the arbiter is the Unity client's DTOs/converters, not the TS server - an endpoint the client never calls cannot break it; a shape the client's converter throws on is broken regardless of TS fidelity.
