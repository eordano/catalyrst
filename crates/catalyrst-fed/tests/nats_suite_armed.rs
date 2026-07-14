#![cfg(not(feature = "nats"))]

#[test]
fn the_nats_live_suite_was_not_built() {
    let _: Option<()> = catalyrst_testgate::unavailable(
        "cargo feature `nats` on catalyrst-fed",
        "tests/nats_live.rs is #![cfg(feature = \"nats\")], so without the feature cargo \
         compiles it to zero tests and the suite reports green having run nothing; \
         run `cargo test -p catalyrst-fed --features nats` with FED_NATS_URL pointing at a broker",
    );
}
