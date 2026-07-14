#![cfg(not(feature = "rpc"))]

#[test]
fn the_rpc_integration_suites_were_not_built() {
    let _: Option<()> = catalyrst_testgate::unavailable(
        "cargo feature `rpc` on catalyrst-social-service",
        "tests/friends_blocks.rs, tests/private_voice_busy.rs, tests/ws_handshake.rs and \
         tests/community_voice_soft_delete.rs are \
         required-features = [\"rpc\"] targets, so without the feature cargo omits them entirely \
         and the crate reports green having never built them; \
         run `cargo test -p catalyrst-social-service --features rpc`",
    );
}
