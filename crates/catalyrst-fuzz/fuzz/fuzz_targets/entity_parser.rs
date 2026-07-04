#![no_main]

use libfuzzer_sys::fuzz_target;
use catalyrst_validator::parse_entity_from_bytes;

fuzz_target!(|data: &[u8]| {
    let _ = parse_entity_from_bytes(data, "fuzz-entity-id");
});
