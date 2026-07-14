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
/// - adding an eighth fork is a diff in *this* crate, where a reviewer will see it.
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
    /// `crates/catalyrst-world-storage/src/auth_chain.rs` — extracts the chain with the
    /// shared extractor but applies its **own** `check_freshness`, which is one-sided.
    WorldStorageCrateLocalAuthChainVerifier,

    /// `crates/catalyrst-scene-state/src/auth.rs` — verifies an auth frame arriving over a
    /// websocket, with a crate-local chain extractor.
    SceneStateWebsocketAuthVerifier,

    /// `crates/catalyrst-signatures/src/auth.rs` — a **full private fork** of the
    /// signed-fetch path. Note also `catalyrst-signatures/src/handlers.rs`'s
    /// `catalyrst_crypto_require_signer`, whose name says it calls `catalyrst-crypto` and
    /// which does not.
    SignaturesLocalAuthChainVerifier,

    /// `crates/catalyrst-pulse/src/handshake.rs` — a handshake verifier that pins the
    /// signed payload to the fixed `"connect"` method and `"/"` path. That pinning is a
    /// *tighter* policy than anything on the shared path; if pulse ever consolidates it
    /// must be preserved as a parameter, not lost.
    PulseCrateLocalHandshakeVerifier,

    /// `crates/catalyrst-explorer-api/src/modules/auth_api/validation.rs` — verifies a
    /// chain **against its own last link**, binding no timestamp, no method and no path.
    ExplorerApiSelfChainVerifier,

    /// `crates/catalyrst-archipelago/src/auth.rs` — redeems a single-use, server-issued
    /// challenge. The only verifier in this list with real replay protection.
    ArchipelagoSingleUseChallengeVerifier,

    /// `crates/catalyrst-server/src/admin/session.rs` — an HMAC session cookie minted after
    /// a SIWE sign-in, re-checked against the live operator allowlist on every request. No
    /// auth chain is involved at all.
    AdminConsoleSiweSessionCookieVerifier,
}

impl NonSharedAuthVerifier {
    /// Every verifier that bypasses the shared one. Pinned by an arity test, so growing the
    /// set is a deliberate act.
    pub const EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE: &'static [Self] = &[
        Self::WorldStorageCrateLocalAuthChainVerifier,
        Self::SceneStateWebsocketAuthVerifier,
        Self::SignaturesLocalAuthChainVerifier,
        Self::PulseCrateLocalHandshakeVerifier,
        Self::ExplorerApiSelfChainVerifier,
        Self::ArchipelagoSingleUseChallengeVerifier,
        Self::AdminConsoleSiweSessionCookieVerifier,
    ];

    /// Whether a credential timestamped in the **future** is rejected.
    ///
    /// The shared path bounds the skew symmetrically: `(now - signed_at).abs() >
    /// expiration_secs` in `catalyrst-crypto/src/signed_fetch.rs`. Two verifiers here do
    /// not, and they do not for two different reasons:
    ///
    /// - `world-storage`'s `check_freshness` is `now - signed_at > expiration_secs`, with
    ///   **no** `.abs()`, so a signature dated arbitrarily far in the future is fresh
    ///   forever. Its own test `freshness_does_not_reject_future_timestamps` currently pins
    ///   that as intended behaviour; inverting that test is the deliverable when it is
    ///   fixed, not a side effect.
    /// - `explorer-api` binds **no timestamp at all** — it passes `None` where the shared
    ///   verifier passes the request's timestamp — so there is nothing to be future-dated
    ///   relative to.
    ///
    /// The remaining five return `true`, each for a reason worth reading: pulse and
    /// signatures and scene-state all apply `.abs()`; archipelago's freshness bound is the
    /// age of a challenge the **server** issued, which a client cannot post-date; and the
    /// admin console's bound is the `exp` inside a cookie the server minted and signed.
    pub fn rejects_future_dated_signatures(self) -> bool {
        match self {
            Self::WorldStorageCrateLocalAuthChainVerifier | Self::ExplorerApiSelfChainVerifier => {
                false
            }
            Self::SceneStateWebsocketAuthVerifier
            | Self::SignaturesLocalAuthChainVerifier
            | Self::PulseCrateLocalHandshakeVerifier
            | Self::ArchipelagoSingleUseChallengeVerifier
            | Self::AdminConsoleSiweSessionCookieVerifier => true,
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
    /// `signatures` and `scene-state` check none of the three; `pulse` rejects a chain whose
    /// first link is not a `SIGNER`, but only after verification and not the other two;
    /// `archipelago` checks the first link's *payload* against the claimed address rather
    /// than its type; `explorer-api` derives its own final authority and checks none of the
    /// three. The admin console returns `false` because there is no auth chain in a session
    /// cookie — the question does not apply, and answering `true` would imply a check that
    /// does not exist.
    pub fn performs_structural_auth_chain_validation(self) -> bool {
        match self {
            Self::WorldStorageCrateLocalAuthChainVerifier => true,
            Self::SceneStateWebsocketAuthVerifier
            | Self::SignaturesLocalAuthChainVerifier
            | Self::PulseCrateLocalHandshakeVerifier
            | Self::ExplorerApiSelfChainVerifier
            | Self::ArchipelagoSingleUseChallengeVerifier
            | Self::AdminConsoleSiweSessionCookieVerifier => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NonSharedAuthVerifier as Verifier;

    /// P14: the set does not grow silently.
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

    /// P14: exactly two verifiers accept a future-dated credential, and we know which.
    #[test]
    fn exactly_world_storage_and_explorer_api_accept_future_dated_credentials() {
        let accepting: Vec<Verifier> = Verifier::EVERY_VERIFIER_THAT_BYPASSES_THE_SHARED_ONE
            .iter()
            .copied()
            .filter(|v| !v.rejects_future_dated_signatures())
            .collect();
        assert_eq!(
            accepting,
            vec![
                Verifier::WorldStorageCrateLocalAuthChainVerifier,
                Verifier::ExplorerApiSelfChainVerifier,
            ]
        );
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

    /// The two weakest members are weak in *different* ways, and neither predicate alone
    /// identifies them. Pinned so that a future "cleanup" cannot collapse the two
    /// predicates into one boolean.
    #[test]
    fn the_two_predicates_are_not_the_same_predicate() {
        assert!(
            Verifier::WorldStorageCrateLocalAuthChainVerifier
                .performs_structural_auth_chain_validation()
                && !Verifier::WorldStorageCrateLocalAuthChainVerifier
                    .rejects_future_dated_signatures()
        );
        assert!(
            !Verifier::PulseCrateLocalHandshakeVerifier.performs_structural_auth_chain_validation()
                && Verifier::PulseCrateLocalHandshakeVerifier.rejects_future_dated_signatures()
        );
    }
}
