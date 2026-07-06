#![no_main]

use libfuzzer_sys::fuzz_target;
use catalyrst_sync::snapshots::parse_snapshot_entities;

fuzz_target!(|data: &[u8]| {
    let _ = parse_snapshot_entities(data);
});
