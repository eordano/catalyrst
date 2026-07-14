use std::fmt;

use catalyrst_crypto::Signer;
use catalyrst_types::EthAddress;

/// A wallet address whose control by the caller was **proven**, by verifying an ADR-44
/// signed-fetch auth chain against this request's method, path, timestamp and metadata.
///
/// # What a value of this type proves
///
/// That `catalyrst_crypto::signed_fetch`'s verification succeeded for *this* request, so
/// the holder of the private key behind this address signed this method, this path, this
/// timestamp and this metadata, within the shared symmetric freshness window.
///
/// # What it does NOT prove
///
/// - **Not authorization.** It says who called, never that they may do anything.
/// - **Not freshness-once.** The shared signed-fetch path has no nonce and no replay
///   cache; the only bound is the tolerance window. A captured signature is replayable
///   inside that window. `catalyrst-fed`'s `Signed<T>` with its single-use nonce is the
///   only mechanism in the workspace with real replay protection.
/// - **Not a role, a tier, a membership or an allowlist entry.** Those are separate
///   lookups against separate stores, each with its own crate-local authority type.
/// - **Not personhood.** A wallet is a key, not a human. It is called
///   `OfTheHumanCaller` to separate it from a *service* identity established by a shared
///   secret (see [`crate::AuthenticatedPlatformServiceIdentity`]), not to assert a person
///   is at the keyboard.
///
/// # How a value is obtained
///
/// Exactly one way: [`Self::from_verified_signed_fetch`].
///
/// # Deliberately absent
///
/// `Deserialize`, `Serialize`, a `ts-rs` `TS` derive, a `utoipa` schema derive,
/// `From<String>`, `FromStr`, `Default`, `PartialEq<str>`, `PartialEq<String>`,
/// `AsRef<str>`, and any `pub fn new`.
///
/// The absence of `PartialEq<str>` is load-bearing and is a deliberate divergence from
/// `catalyrst_crypto::Signer`, which has it (`crates/catalyrst-crypto/src/signer.rs`).
/// Those impls are what make an allowlist comparison such as
/// `state.admin_addresses.iter().any(|a| a == &signer)` compile against a bare
/// `Vec<String>`. The replacement is [`crate::ConfiguredWalletAllowlist::contains`],
/// which takes this type and nothing else, so a request-body field cannot be handed to it.
///
/// The absence of `Deserialize` is how "a claim is not an identity" is expressed: see
/// [`crate::ClaimedWalletAddressNobodyHasVerified`], which has `Deserialize` and has no
/// conversion into this type.
// The doc above deliberately does not spell the ts-rs derive path with its double colon.
// catalyrst/sites/scripts/generated-artefacts.mts decides whether a crate emits TypeScript bindings
// by substring-matching that exact spelling anywhere under src/, comments included, and a
// crate it believes emits bindings must declare [package.metadata.generated]. This crate
// emits nothing and must never appear in the generated index. Prose only, not a workaround
// for a real binding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VerifiedWalletAddress(EthAddress);

impl VerifiedWalletAddress {
    /// The only constructor in the workspace. This is **the chokepoint**.
    ///
    /// It takes [`catalyrst_crypto::Signer`] **by value**, and that is the whole of the
    /// argument:
    ///
    /// 1. This type has a private field and no other constructor -- no `From<String>`, no
    ///    `FromStr`, no `Deserialize`, no `Default`, no `new`.
    /// 2. `Signer` also has a private field, and its only constructor is
    ///    `pub(crate) Signer::from_verified_chain` in
    ///    `crates/catalyrst-crypto/src/signer.rs`, reachable from exactly two places --
    ///    the two `Ok(Signer::from_verified_chain(&chain.signer))` returns at the end of
    ///    `validate_signature` and its async twin in
    ///    `crates/catalyrst-crypto/src/signed_fetch.rs`, both immediately after
    ///    `verify_auth_chain` returned success.
    /// 3. The one escape hatch, `Signer::unchecked_for_test`, is
    ///    `#[cfg(any(test, feature = "test-signer"))]`, and no crate in the workspace
    ///    enables `test-signer`.
    ///
    /// Therefore, outside `catalyrst-crypto`'s own test build, a value of this type cannot
    /// exist unless `verify_auth_chain` succeeded against this request. Unforgeability is
    /// **inherited** from `Signer`, not re-asserted here.
    ///
    /// The address is lowercased on the way in, as `Signer` already does, so every
    /// comparison downstream is case-stable.
    ///
    /// # This is a convention with a script attached, not a total guarantee
    ///
    /// A second `pub fn` returning `Self` added to this file would widen the chokepoint
    /// silently. `tests/source_discipline.rs` asserts that no such function exists. That
    /// test is the guard; the compiler is not.
    pub fn from_verified_signed_fetch(signer: Signer) -> Self {
        Self(signer.as_str().to_ascii_lowercase())
    }

    /// The lowercased `0x...` text, for logging, for an audit column, and for a SQL bind
    /// parameter.
    ///
    /// Named at length so that a reviewer sees the exit from the type system at every use
    /// site. Comparing this text to another string is exactly the pattern this type
    /// exists to remove -- compare with [`crate::ConfiguredWalletAllowlist::contains`]
    /// or with `PartialEq` against another `VerifiedWalletAddress`.
    pub fn as_lowercased_hex_text(&self) -> &str {
        &self.0
    }

    /// The single, loud, documented escape hatch -- and it is **unreachable outside this
    /// crate**.
    ///
    /// `#[cfg(test)]` with no cargo feature attached, unlike
    /// `catalyrst_crypto::Signer::unchecked_for_test`, whose `feature = "test-signer"`
    /// arm could in principle be turned on by workspace feature unification. A `cfg(test)`
    /// item is compiled only when *this* crate's own unit tests are built, so no other
    /// crate -- and not even this crate's own `tests/` directory, which is a separate
    /// compilation unit -- can reach it.
    ///
    /// Its body is deliberately byte-identical to
    /// [`Self::from_verified_signed_fetch`]'s, so that a test
    /// exercising this hatch is exercising the same normalization the real mint performs.
    /// If one changes, change both.
    #[cfg(test)]
    pub(crate) fn unchecked_for_unit_tests_within_this_crate_only(address: &str) -> Self {
        Self(address.to_ascii_lowercase())
    }
}

impl fmt::Display for VerifiedWalletAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The strongest available demonstration of the chokepoint is a negative one, and it
    // is why every test below goes through the `#[cfg(test)]` hatch rather than the real
    // mint: **this crate's own unit tests cannot call the real mint.**
    //
    // Calling `from_verified_signed_fetch` requires a
    // `catalyrst_crypto::Signer`, and there is no way to obtain one here.
    // `Signer::from_verified_chain` is `pub(crate)` to `catalyrst-crypto`, and
    // `Signer::unchecked_for_test` is `#[cfg(any(test, feature = "test-signer"))]` -- the
    // `test` arm is `catalyrst-crypto`'s own test build, not ours, and this crate
    // deliberately does **not** enable `test-signer`, because a dev-dependency feature
    // unifies across a `cargo test --workspace` run and would arm that hatch fleet-wide.
    // `tests/source_discipline.rs` asserts that this crate's Cargo.toml never acquires it.

    #[test]
    fn minting_lowercases_the_address() {
        let verified = VerifiedWalletAddress::unchecked_for_unit_tests_within_this_crate_only(
            "0xABCDEF0123456789ABCDEF0123456789ABCDEF01",
        );
        assert_eq!(
            verified.as_lowercased_hex_text(),
            "0xabcdef0123456789abcdef0123456789abcdef01"
        );
    }

    #[test]
    fn two_mintings_of_the_same_address_in_different_cases_are_equal() {
        let lower = VerifiedWalletAddress::unchecked_for_unit_tests_within_this_crate_only("0xaa");
        let upper = VerifiedWalletAddress::unchecked_for_unit_tests_within_this_crate_only("0xAA");
        assert_eq!(lower, upper);
    }

    #[test]
    fn display_renders_the_lowercased_text() {
        let verified =
            VerifiedWalletAddress::unchecked_for_unit_tests_within_this_crate_only("0xAb");
        assert_eq!(format!("{verified}"), "0xab");
    }

    // Negative construction rules, recorded next to the type. They cannot be written as
    // failing unit tests -- they are absences -- so the mechanism that actually runs is the
    // source-text scan in `tests/source_discipline.rs`. Each of the following is a compile
    // error today, and there is no in-crate way to make any of them compile:
    //
    //   let _: VerifiedWalletAddress = "0xdead".into();            // E0277
    //   let _ = VerifiedWalletAddress::from("0xdead".to_string()); // E0599
    //   let _: VerifiedWalletAddress = "0xdead".parse().unwrap();  // E0277
    //   let _ = VerifiedWalletAddress::default();                  // E0599
    //   serde_json::from_str::<VerifiedWalletAddress>("\"0x\"");   // E0277
    //   let _ = VerifiedWalletAddress::new("0xdead");              // E0599
    //   if verified == "0xdead" { }                                                // E0277
    //   allowlist_of_plain_strings.contains(&verified);                            // E0308
}
