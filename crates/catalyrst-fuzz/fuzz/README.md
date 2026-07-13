# catalyrst fuzz harness

Arbitrary-input fuzzing of catalyrst's parse / verify paths, built with [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) (libFuzzer under the hood). Separate from the parent `catalyrst-fuzz` crate (which holds `loom` interleaving tests for concurrency, not input fuzzing) and excluded from the top-level workspace: cargo-fuzz needs nightly + the libFuzzer linker, which would break the stable-toolchain `cargo build --workspace`. The exclusion is `crates/catalyrst-fuzz/fuzz` in the root `[workspace] exclude` list.

## Setup (one-time)

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Run a target

```bash
cd crates/catalyrst-fuzz
cargo +nightly fuzz run entity_parser
# Available targets:
#   entity_parser         - catalyrst_validator::parse_entity_from_bytes
#   auth_chain_decode     - serde_json::from_slice::<AuthChain> + is_valid_auth_chain
#   content_hash_verify   - catalyrst_hashing::{verify_hash, hash_bytes, hash_bytes_v1}
#   snapshot_parser       - catalyrst_sync::snapshots::parse_snapshot_entities
#   builder_signed_fetch  - catalyrst_builder signed-fetch header parse +
#                           curation authorize_admin gate (no panic; garbage
#                           never authorizes via the signature branch)
```

Each target keeps a growing corpus under `fuzz/corpus/<target>/` and writes crashing inputs to `fuzz/artifacts/<target>/`; both gitignored. Continuous fuzzing: `cargo fuzz run <target> -- -max_total_time=300` caps each invocation; CI could run this nightly on a separate runner. NOT wired into the main CI workflow (workflow uses stable Rust).
