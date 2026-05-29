# catalyrst fuzz harness

Real arbitrary-input fuzzing of catalyrst's parse / verify paths, built
with [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html)
(libFuzzer under the hood).

This sub-project is intentionally separate from the parent
`catalyrst-fuzz` crate (which contains `loom` interleaving tests for
concurrency correctness, not input fuzzing) and is excluded from the
top-level workspace so the stable-toolchain `cargo build --workspace`
keeps working.

## Setup (one-time)

```bash
# Nightly Rust is required by cargo-fuzz / libfuzzer-sys:
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Run a target

```bash
cd crates/catalyrst-fuzz
cargo +nightly fuzz run entity_parser
# Available targets:
#   entity_parser         — catalyrst_validator::parse_entity_from_bytes
#   auth_chain_decode     — serde_json::from_slice::<AuthChain> + is_valid_auth_chain
#   content_hash_verify   — catalyrst_hashing::{verify_hash, hash_bytes, hash_bytes_v1}
#   snapshot_parser       — catalyrst_sync::snapshots::parse_snapshot_entities
```

Each target keeps a growing corpus under `fuzz/corpus/<target>/` and writes
crashing inputs to `fuzz/artifacts/<target>/`. Both directories are
gitignored.

## Continuous fuzzing

`cargo fuzz run <target> -- -max_total_time=300` to cap each invocation;
the CI workflow could run this nightly on a separate runner. NOT wired
into the main CI workflow (workflow uses stable Rust).

## Why this lives in a separate sub-Cargo project

`cargo-fuzz` projects need nightly + the libFuzzer linker, which would
fail on the stable-toolchain workspace build. The `fuzz/` subdirectory is
excluded from the workspace via `crates/catalyrst-fuzz/fuzz` in the root
`[workspace] exclude` list.
