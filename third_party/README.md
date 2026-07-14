# third_party

## rusty_enet

`rusty_enet/` is **crates.io `rusty_enet` 0.4.0, verbatim, plus the single 3-constant patch in
`rusty_enet.patch`** — not a fork we maintain. The patch aligns the ENet protocol header-flag
constants with the *decentraland-pulse* ENet variant that the C# Pulse server and the
bevy-explorer PR #919 native client speak. Stock ENet 1.3.x and the decentraland-pulse variant
misparse each other's packet headers, so native-ENet peers cannot connect across the two.

Wired in via the workspace `Cargo.toml`:

```toml
[patch.crates-io]
rusty_enet = { path = "third_party/rusty_enet" }
```

This keeps `rusty_enet = "0.4"` as the canonical dependency everywhere; the patch only swaps the
build's source. The WebTransport transport is unaffected (it does not use these flags and already
interoperates byte-for-byte).

To refresh against a newer `rusty_enet`: re-copy the crate from the registry, re-apply
`rusty_enet.patch` (or the three edits it documents), and bump the vendored `Cargo.toml` version.
The Nix/hermetic build consumes the same vendored path, so no separate patching step is required.
