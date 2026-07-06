# Building and testing

Written against `flake.nix`, `Cargo.toml`, and the test crates.

## Toolchain facts not in the README

- Stable Rust; no pinned toolchain file, but the flake pins by shell: `nix develop`
  (default, nixpkgs stable ~1.95), `.#ci` (rust-overlay 1.97.0, the CI toolchain),
  `.#gpu` (adds the CUDA/vulkan loader env). `protoc` required (dcl-rpc protobuf
  codegen in social-rpc/quests), `cmake` for abgen's native deps.
- The HTTP stack is rustls, but `openssl-sys` arrives transitively via the Helios consensus
  light-client - a system OpenSSL (+`pkg-config`) is needed at compile time. Build with
  `OPENSSL_NO_VENDOR=1` to link the system library.
- The `nats` cargo feature (federation live gossip) is off by default; build
  `-p catalyrst-fed --features nats` (and the embedding service binary with the same feature) to
  get the real transport.

On NixOS, the flake devShell has the full toolchain (cargo, rustc, rustfmt, clippy, rust-analyzer,
protoc, cmake, openssl, turbojpeg) with `OPENSSL_NO_VENDOR`, `RUSTY_V8_ARCHIVE` (pinned
librusty_v8), and `TURBOJPEG_LIB` preset:

```bash
nix develop            # or: nix develop -c cargo check --workspace
nix build .#catalyrst          # catalyrst-live (pinned, reproducible)
nix build .#catalyrst-all      # ~13-binary bundle package
```

Keep `CARGO_HOME` on persistent disk if your /tmp is small/volatile. Nix builds compile only
committed/tracked files - an untracked new file that `cargo` uses will be missing from a
`nix build`. Track files at creation.

Flake pin/patch rationale:

- Helios: all seven `helios-*` crates come from one git revision - a single `outputHashes` entry.
- archipelago-workers (Node): upstream pins `uWebSockets.js` v20.43 whose prebuilt binding maxes
  out at Node 21 ABI; the flake swaps in v20.67 (Node 24 ABI) post-build and carries a vendored
  `nixos/archipelago-package-lock.json` (upstream only tracks `yarn.lock`; `buildNpmPackage` needs
  an npm lockfile to hash). Workspace builds are shelled per-workspace - npm has no
  `yarn workspaces run build` equivalent.
- `librusty_v8` pinned via `crates/catalyrst-scene-state/nix/librusty_v8.nix` (scene-state embeds
  a JS runtime).
- `doCheck = false` across flake packages - tests run via cargo, not inside nix builds.

## Test surfaces (beyond `cargo test`)

| Harness | What it proves | How |
|---|---|---|
| unit tests | per-crate logic incl. the parity canaries (snapshot progression vectors, boundary double-count, pointer-changes URL resolution, wire-shape regression tests) | `cargo test --workspace` |
| `catalyrst-conformance` | live A/B parity of two catalyst hosts; bootstraps inputs from the baseline, diffs `/content` + `/lambdas` | `cargo run -p catalyrst-conformance -- --baseline <ref> --candidate <ours>` |
| `catalyrst-conformance-capture` / `-replay` | recorded-fixture parity, offline/CI-friendly; fixtures in `crates/catalyrst-conformance/fixtures/`, state-dependent fields masked by `volatility.toml` + per-fixture `volatile_paths` | capture once against a real peer, replay forever |
| `catalyrst-oracle-tests` | foundation crates (hashing, crypto, validator, storage) reproduce real vectors extracted from a live catalyst DB - CIDs, auth chains, entity parses, on-disk sha1-prefix paths | `cargo run -p catalyrst-oracle-tests --bin extract` (needs `CATALYRST_ORACLE_DB_URL` + `CATALYRST_ORACLE_CONTENT_ROOT`), then `cargo test -p catalyrst-oracle-tests -- --ignored`; `test-vectors/` is generated, not committed |
| `scripts/schemathesis/` | property-based fuzzing of a running server against [`docs/openapi.yaml`](./openapi.yaml); custom checks for 5xx, schema conformance, CORS, error-body shape | `scripts/schemathesis/run.sh --target http://127.0.0.1:5141` |
| `catalyrst-fuzz`, `catalyrst-bench` | fuzz/stress harnesses; criterion benches for hot paths (bench persists previous results so delta-p50/delta-p99 columns show regressions) | `cargo bench` etc. |
| federation gossip | in-process loop test (`tests/gossip_loop.rs`, broker-free); `nats_live` runs against a real broker only when `FED_NATS_URL` is set | see [federation.md](./federation.md) |
| abgen gates | fork-parity byte ratio, live-mode structural diff, three-byte-mode render gate | see [asset-bundles.md](./asset-bundles.md) - mandatory after touching abgen builder/live/texture/animation code |

Three escalating sources of truth, cheapest first: unit tests pin the invariants; conformance/
oracle pin wire and byte behavior against real reference data; for client-facing questions the
arbiter is the Unity client's DTOs/converters, not the TS server - an endpoint the client never
calls cannot break it, and a shape the client's converter throws on is broken regardless of how
faithfully it mirrors the TS code. Per-service audits: `ff400cab^:catalyrst/docs/verification/`.
