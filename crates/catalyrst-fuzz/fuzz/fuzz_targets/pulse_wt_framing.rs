#![no_main]

//! Coverage-guided fuzzing of the WebTransport wire-facing decoders — the parser most exposed to
//! hostile browser bytes. `parse_datagram` on arbitrary input and the `StreamFrameReader`
//! reassembler driven with arbitrary chunk boundaries and length prefixes must never panic.
//!
//!   cargo +nightly fuzz run pulse_wt_framing

use libfuzzer_sys::fuzz_target;

use catalyrst_pulse::transport::webtransport::framing::{parse_datagram, StreamFrameReader};

fuzz_target!(|data: &[u8]| {
    // Datagram header parsing on raw bytes.
    let _ = parse_datagram(data);

    // Stream reassembly: split the input into chunks (first byte of the remainder picks the next
    // chunk size) so the fuzzer explores frame boundaries that straddle the length prefix.
    let mut reader = StreamFrameReader::new(4096);
    let mut rest = data;
    while !rest.is_empty() {
        let take = (rest[0] as usize % 64) + 1;
        let (chunk, tail) = rest.split_at(take.min(rest.len()));
        reader.append(chunk);
        loop {
            match reader.try_read() {
                Ok(Some(msg)) => assert!(msg.len() <= 4096),
                Ok(None) => break,
                Err(_) => break, // overrun: buffer dropped, keep feeding
            }
        }
        rest = tail;
    }
});
