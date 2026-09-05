/// Every verifier in the workspace that mints an identity **without** going through
/// `catalyrst_crypto::signed_fetch`.
///
/// # What a value of this type proves
///
/// Nothing about any request. This enum has no identity inside it, mints nothing, and is
/// attached to no credential. It exists so that:
///
/// - the set of forks is **enumerable** rather than folklore;
/// - each fork's weaknesses are stated as **facts about the fleet**, asserted by a test,
///   rather than as a comment in one file that nobody reads;
/// - adding another fork is a diff in *this* crate, where a reviewer will see it.
///
/// # What it does NOT prove
///
/// That any of these verifiers is safe, that any of them is going to be migrated, or that
/// the two predicates below are the only differences between them. They are the two
/// differences that have bitten, not an exhaustive audit.
///
/// # How a value is obtained
///
/// By naming a variant. There is deliberately **no constructor** taking a string or an
/// enum-plus-string, because such a constructor would be a fresh hole of exactly the kind
/// this crate exists to close: a caller could mint an identity by asserting which verifier
/// it "used".
///
/// # Deliberately not `#[non_exhaustive]`
///
/// A new fork must break every exhaustive match, including the two predicates below, which
/// forces whoever adds it to state its freshness and structural-validation behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NonSharedAuthVerifier {
    /// `crates/catalyrst-world-storage/src/auth_chain.rs` -- extracts the chain with the
    /// shared extractor but applies its **own** `check_freshness`, a local copy of the
    /// shared symmetric `.abs()` bound.
    WorldStorageCrateLocalAuthChainVerifier,

    /// `crates/catalyrst-scene-state/src/auth.rs` -- verifies an auth frame arriving over a
    /// websocket, with a crate-local chain extractor.
    SceneStateWebsocketAuthVerifier,

    /// `crates/catalyrst-pulse/src/handshake.rs` -- a handshake verifier that pins the
    /// signed payload to the fixed `"connect"` method and `"/"` path. That pinning is a
    /// *tighter* policy than anything on the shared path; if pulse ever consolidates it
    /// must be preserved as a parameter, not lost.
    PulseCrateLocalHandshakeVerifier,

    /// `crates/catalyrst-explorer-api/src/modules/auth_api/validation.rs` -- verifies a
    /// chain **against its own last link**, binding no timestamp, no method and no path.
    ExplorerApiSelfChainVerifier,

    /// `crates/catalyrst-archipelago/src/auth.rs` -- redeems a single-use, server-issued
    /// challenge. The only verifier in this list with real replay protection.
    ArchipelagoSingleUseChallengeVerifier,

    /// `crates/catalyrst-server/src/admin/session.rs` -- an HMAC session cookie minted after
    /// a SIWE sign-in, re-checked against the live operator allowlist on every request. No
    /// auth chain is involved at all.
    AdminConsoleSiweSessionCookieVerifier,

    /// `crates/catalyrst-fed/src/sig.rs` -- `Signed<T>::signer()`/`verify()`, the federation
    /// write envelope. EIP-712-*styled* but not EIP-712: the digest is SHA-256 over
    /// `\x19\x01` + a SHA-256 "domain separator" + `encode_struct()` + nonce + `signed_at`,
    /// not the keccak typed-data encoding -- a wire format of its own. `verify()` recovers
    /// the address from that digest, requires it to equal an **expected** signer
    /// (case-insensitively), and bounds skew asymmetrically: at most 30 s in the future,
    /// 5 min in the past. Two facts worth knowing before trusting it: `signer()` alone
    /// binds no expected signer and no freshness -- only `verify()` is a verifier; and the
    /// nonce widens the digest but is stored nowhere, so it is NOT replay protection --
    /// within the window the envelope replays, same as the shared signed-fetch path.
    FedEip712StyleEnvelopeVerifier,
}

impl NonSharedAuthVerifier {
    /// Every verifier that bypasses the shared one. Pinned by an arity test, so growing the
    /// set is a deliberate act.
    pub const EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE: &'static [Self] = &[
        Self::WorldStorageCrateLocalAuthChainVerifier,
        Self::SceneStateWebsocketAuthVerifier,
        Self::PulseCrateLocalHandshakeVerifier,
        Self::ExplorerApiSelfChainVerifier,
        Self::ArchipelagoSingleUseChallengeVerifier,
        Self::AdminConsoleSiweSessionCookieVerifier,
        Self::FedEip712StyleEnvelopeVerifier,
    ];

    /// Whether a credential timestamped in the **future** is rejected.
    ///
    /// The shared path bounds the skew symmetrically: `(now - signed_at).abs() >
    /// expiration_secs` in `catalyrst-crypto/src/signed_fetch.rs`. One verifier here does
    /// not:
    ///
    /// - `explorer-api` binds **no timestamp at all** -- it passes `None` where the shared
    ///   verifier passes the request's timestamp -- so there is nothing to be future-dated
    ///   relative to.
    ///
    /// `world-storage` used to be a second `false`: its local `check_freshness` had no
    /// `.abs()`, so a signature dated arbitrarily far in the future was fresh forever. That
    /// was fixed by adding the shared path's symmetric bound and inverting the test that
    /// pinned the old behaviour (`freshness_rejects_far_future_timestamps` now pins the
    /// fix).
    ///
    /// The rest return `true`, each for a reason worth reading: world-storage and pulse
    /// and scene-state all apply `.abs()`; archipelago's freshness bound is the age of a
    /// challenge the **server** issued, which a client cannot post-date; the admin
    /// console's bound is the `exp` inside a cookie the server minted and signed; and
    /// fed's `verify()` caps future skew explicitly at `MAX_SKEW_FUTURE_SECS` (30 s) --
    /// though only `verify()` does; `Signed::signer()` alone binds no freshness at all.
    pub fn rejects_future_dated_signatures(self) -> bool {
        match self {
            Self::ExplorerApiSelfChainVerifier => false,
            Self::WorldStorageCrateLocalAuthChainVerifier
            | Self::SceneStateWebsocketAuthVerifier
            | Self::PulseCrateLocalHandshakeVerifier
            | Self::ArchipelagoSingleUseChallengeVerifier
            | Self::AdminConsoleSiweSessionCookieVerifier
            | Self::FedEip712StyleEnvelopeVerifier => true,
        }
    }

    /// Whether the verifier performs the three **structural** auth-chain checks that
    /// `catalyrst_crypto::signed_fetch::extract_auth_chain` performs:
    ///
    /// 1. a `SIGNER` link may only appear at index 0;
    /// 2. the first link must be a `SIGNER`;
    /// 3. every non-first link must carry a non-empty signature.
    ///
    /// Only `world-storage` returns `true`, and only because it delegates extraction to the
    /// shared function outright. The rest hand-roll extraction and check some subset:
    /// `scene-state` checks none of the three; `pulse` rejects a chain whose first link is
    /// not a `SIGNER`, but only after verification and not the other two; `archipelago`
    /// checks the first link's *payload* against the claimed address rather than its type;
    /// `explorer-api` derives its own final authority and checks none of the three. The
    /// admin console and fed return `false` because neither involves an auth chain -- a
    /// session cookie and a single-signature typed envelope respectively -- so the question
    /// does not apply, and answering `true` would imply a check that does not exist.
    pub fn performs_structural_auth_chain_validation(self) -> bool {
        match self {
            Self::WorldStorageCrateLocalAuthChainVerifier => true,
            Self::SceneStateWebsocketAuthVerifier
            | Self::PulseCrateLocalHandshakeVerifier
            | Self::ExplorerApiSelfChainVerifier
            | Self::ArchipelagoSingleUseChallengeVerifier
            | Self::AdminConsoleSiweSessionCookieVerifier
            | Self::FedEip712StyleEnvelopeVerifier => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NonSharedAuthVerifier as Verifier;

    /// P14: the set does not grow (or shrink) silently. It shrank from seven when the
    /// signatures crate's private fork was consolidated onto the shared signed-fetch path,
    /// and returned to seven when the claim to enumerate ALL non-shared verifiers was made
    /// true by recording fed's `Signed<T>` envelope, which had been omitted.
    #[test]
    fn there_are_exactly_seven_verifiers_bypassing_the_shared_one() {
        assert_eq!(
            Verifier::EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE.len(),
            7,
            "a verifier was added or removed; update the registry, both predicates, and \
             docs/auth.md in the same commit"
        );
    }

    #[test]
    fn the_registry_lists_each_verifier_exactly_once() {
        let mut seen = Verifier::EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE.to_vec();
        let before = seen.len();
        seen.sort_by_key(|v| format!("{v:?}"));
        seen.dedup();
        assert_eq!(seen.len(), before, "a verifier is listed twice");
    }

    /// P14: exactly one verifier accepts a future-dated credential, and we know which.
    /// (world-storage was the second until its `check_freshness` gained the shared
    /// symmetric bound.)
    #[test]
    fn only_explorer_api_accepts_future_dated_credentials() {
        let accepting: Vec<Verifier> = Verifier::EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE
            .iter()
            .copied()
            .filter(|v| !v.rejects_future_dated_signatures())
            .collect();
        assert_eq!(accepting, vec![Verifier::ExplorerApiSelfChainVerifier]);
    }

    /// Only the verifier that reuses the shared extractor gets the shared extractor's
    /// structural checks. Stated as a fact so that a consolidation which quietly drops them
    /// shows up here.
    #[test]
    fn only_world_storage_reuses_the_shared_structural_validation() {
        let structural: Vec<Verifier> = Verifier::EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE
            .iter()
            .copied()
            .filter(|v| v.performs_structural_auth_chain_validation())
            .collect();
        assert_eq!(
            structural,
            vec![Verifier::WorldStorageCrateLocalAuthChainVerifier]
        );
    }

    /// The predicates disagree on real members, so neither can be derived from the other.
    /// Pinned so that a future "cleanup" cannot collapse the two predicates into one
    /// boolean.
    #[test]
    fn the_two_predicates_are_not_the_same_predicate() {
        assert!(
            !Verifier::PulseCrateLocalHandshakeVerifier.performs_structural_auth_chain_validation()
                && Verifier::PulseCrateLocalHandshakeVerifier.rejects_future_dated_signatures()
        );
        assert!(
            !Verifier::ExplorerApiSelfChainVerifier.performs_structural_auth_chain_validation()
                && !Verifier::ExplorerApiSelfChainVerifier.rejects_future_dated_signatures()
        );
    }
}
