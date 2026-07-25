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
- archipelago-workers (Node): upstream pins `uWebSockets.js` v20.43 (prebuilt binding maxes at Node 21 ABI); the flake swaps in v20.67 (Node 24 ABI) post-build and vendors `nixos/archipelago-package-lock.json` (upstream tracks `yarn.lock`; `buildNpmPackage` needs an npm lockfile).
- `librusty_v8` pinned via `crates/catalyrst-scene-state/nix/librusty_v8.nix` (scene-state embeds a JS runtime).
- `doCheck = false` across flake packages - tests run via cargo, not nix builds.

## Test surfaces

| Harness | What it proves | How |
|---|---|---|
| unit tests | per-crate logic incl. parity canaries (snapshot progression vectors, boundary double-count, pointer-changes URL resolution, wire-shape regressions) | `cargo test --workspace` |
| `catalyrst-conformance` | live A/B parity of two hosts; bootstraps inputs from the baseline, diffs `/content`, `/lambdas` | `cargo run -p catalyrst-conformance -- --baseline <ref> --candidate <ours>` |
| `catalyrst-conformance-capture` / `-replay` | recorded-fixture parity, offline/CI-friendly; fixtures in `crates/catalyrst-conformance/fixtures/`; state-dependent fields masked by `volatility.toml`, per-fixture `volatile_paths` | capture once against a peer, replay forever |
| `catalyrst-oracle-tests` | foundation crates (hashing, crypto, validator, storage) reproduce vectors from a live catalyst DB - CIDs, auth chains, entity parses, on-disk sha1-prefix paths | `cargo run -p catalyrst-oracle-tests --bin extract` (needs `CATALYRST_ORACLE_DB_URL`, `CATALYRST_ORACLE_CONTENT_ROOT`), then `cargo test -p catalyrst-oracle-tests -- --ignored`; `test-vectors/` generated, not committed |
| `scripts/schemathesis/` | property-based fuzzing of a running server against [`docs/openapi.yaml`](./openapi.yaml); checks for 5xx, schema conformance, CORS, error-body shape | `scripts/schemathesis/run.sh --target http://127.0.0.1:5141` |
| `catalyrst-fuzz`, `catalyrst-bench` | fuzz/stress harnesses; criterion benches for hot paths (persisted previous results give delta-p50/p99 regression columns) | `cargo bench` etc. |
| federation gossip | in-process loop test (`tests/gossip_loop.rs`, broker-free); `nats_live` needs a broker, skips when `FED_NATS_URL` unset | see [federation.md](./federation.md) |
| abgen gates | fork-parity byte ratio, live-mode structural diff, render gates | upstream `decentraland/abgen` - see [architecture.md](./architecture.md) |

Three sources of truth, cheapest first: unit tests pin invariants; conformance/oracle pin wire, byte behavior against reference data; for client-facing questions the arbiter is the Unity client's DTOs/converters, not the TS server - an endpoint the client never calls cannot break it; a shape the client's converter throws on is broken regardless of TS fidelity.
