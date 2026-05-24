#![no_main]
//! Fuzz `catalyrst_validator::parse_entity_from_bytes`.
//!
//! Goal: surface inputs that panic, infinite-loop, or allocate unboundedly
//! while decoding an arbitrary byte buffer as an entity JSON document.
//! The parser is the trust boundary between an untrusted HTTP upload and
//! the in-memory `Entity` struct, so any panic here is a DoS vector.

use libfuzzer_sys::fuzz_target;
use catalyrst_validator::parse_entity_from_bytes;

fuzz_target!(|data: &[u8]| {
    // The second arg is the (caller-supplied) entity id; the parser must
    // tolerate any string here, including non-utf8-clean / empty / huge,
    // so we feed it a stable known value and let the fuzzer mutate `data`.
    let _ = parse_entity_from_bytes(data, "fuzz-entity-id");
});
