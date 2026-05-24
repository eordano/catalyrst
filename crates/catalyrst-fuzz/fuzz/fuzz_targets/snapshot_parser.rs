#![no_main]
//! Fuzz `catalyrst_sync::snapshots::parse_snapshot_entities`.
//!
//! This is the only public entry point that exercises BOTH the gzip
//! decompression path (with the new 4 GiB read cap) AND the per-line
//! JSON `SyncDeployment` decode. A peer can serve us this payload over
//! HTTP, so a panic is a remotely-triggerable DoS.
//!
//! (`decompress_snapshot` itself is private — we drive it indirectly
//! through `parse_snapshot_entities`, which is the actual call site on
//! the sync hot path anyway.)

use libfuzzer_sys::fuzz_target;
use catalyrst_sync::snapshots::parse_snapshot_entities;

fuzz_target!(|data: &[u8]| {
    let _ = parse_snapshot_entities(data);
});
