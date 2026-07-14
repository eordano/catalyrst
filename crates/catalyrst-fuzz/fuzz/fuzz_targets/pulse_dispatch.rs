#![no_main]

//! Coverage-guided fuzzing of the Pulse packet-handling path — the continuous, out-of-gate
//! sibling of the in-tree proptest suite in `catalyrst_pulse::fuzz`. The first input byte selects
//! the peer state (pending-auth vs authenticated) so both the handshake/decode surface and the
//! gameplay handlers are reachable; the rest is the raw packet body. `dispatch` ignores the ENet
//! channel, so no channel byte is spent. The invariant is that no input panics the server.
//!
//!   cargo +nightly fuzz run pulse_dispatch

use libfuzzer_sys::fuzz_target;

use catalyrst_pulse::fuzz::{authenticated_server, drive, pending_server, FUZZ_PEER};

fuzz_target!(|data: &[u8]| {
    let (selector, body) = match data.split_first() {
        Some((s, rest)) => (*s, rest),
        None => (0u8, &[][..]),
    };
    // Even selector → an authenticated peer (gameplay handlers); odd → a pending peer (handshake
    // decode + attempt policy + the untracked/pending guards).
    let mut server = if selector & 1 == 0 {
        authenticated_server(FUZZ_PEER)
    } else {
        pending_server(FUZZ_PEER)
    };
    let _ = drive(&mut server, FUZZ_PEER, 1, body);
});
