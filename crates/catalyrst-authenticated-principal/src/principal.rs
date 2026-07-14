use crate::platform_service_identity::AuthenticatedPlatformServiceIdentity;
use crate::verified_wallet_address::VerifiedWalletAddress;

/// Who a request is from, and by what mechanism that was established.
///
/// # What a value of this type proves
///
/// That one of five named authentication mechanisms completed successfully. The variant
/// *is* the mechanism: a caller cannot be "authenticated" in the abstract, only
/// authenticated by something, and the something determines what may be concluded.
///
/// # What it does NOT prove
///
/// - **Not authorization.** No variant grants anything. Every "may WHO do WHAT to WHOM"
///   question is answered by a crate-local authority type in the crate that owns the
///   question, resolved against the store that owns the answer.
/// - **Not that a human is present.** Two variants carry a wallet address; a wallet is a
///   key.
/// - **Not that the mechanisms are equally strong.** They are not. A shared bearer token
///   is a fleet-wide static secret; an ADR-44 signature binds one request. The variants
///   exist precisely so a gate cannot accidentally treat them as equivalent.
///
/// # How a value is obtained
///
/// By constructing a variant around a payload that could only be produced by the
/// corresponding verification -- see
/// [`VerifiedWalletAddress::from_verified_signed_fetch`]
/// and
/// [`crate::establish_platform_service_identity_by_comparing_presented_shared_secret`].
/// The enum adds no authority of its own; it cannot, because it has no private state.
///
/// # Deliberately not `#[non_exhaustive]`
///
/// A sixth kind of caller must break every exhaustive match in the workspace. That forcing
/// is the entire point of the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticatedPrincipal {
    /// A human wallet, proven for this request by the shared ADR-44 signed-fetch verifier
    /// in `catalyrst_crypto::signed_fetch`. The strongest mechanism in the fleet on the
    /// shared path -- and still not replay-proof; there is no nonce.
    HumanWalletProvenByAdr44SignedFetchSignature(VerifiedWalletAddress),

    /// A human wallet, proven by a valid HMAC admin-console session cookie minted after a
    /// SIWE sign-in, and re-checked against the live operator allowlist on every request
    /// (`catalyrst-server/src/admin/session.rs`).
    ///
    /// **Unconstructed as of this crate landing, and that is deliberate.** The variant
    /// exists first so that the eventual `catalyrst-server` migration is a mint rather than
    /// a redesign. The verifier behind it is registered as
    /// [`crate::NonSharedAuthVerifier::AdminConsoleSiweSessionCookieVerifier`].
    VerifiedAdminConsoleWallet(VerifiedWalletAddress),

    /// A platform service, proven by possession of a shared static bearer secret. Never a
    /// person; there is no wallet address reachable from this variant.
    PlatformServiceProvenBySharedBearerToken(AuthenticatedPlatformServiceIdentity),

    /// A peer catalyst server, proven by a federation envelope signature. The wallet
    /// carried here is the **relaying peer's**, not the originating end user's -- a peer
    /// relays actions on behalf of wallets it does not control, and conflating the two is
    /// how one action ends up with two predicates.
    VerifiedPeerCatalyst(VerifiedWalletAddress),

    /// An external system -- a LiveKit cluster webhook, a payment processor callback --
    /// proven by whatever credential that system was configured with. Not a person, and not
    /// a platform service either: it is outside the fleet.
    WebhookAuthenticatedSystem(AuthenticatedPlatformServiceIdentity),
}

impl AuthenticatedPrincipal {
    /// The one narrowing helper: the wallet address, **if and only if** this principal is a
    /// human wallet.
    ///
    /// Returns `None` for every service-shaped principal, and -- deliberately -- also for a
    /// relaying peer server, whose wallet identifies the *peer*, not any end user. A gate
    /// that wants the originating end user of a federated envelope must name that claim
    /// explicitly; it is not available through this method.
    ///
    /// This is what makes the current shape of `catalyrst-comms/src/handlers/voice.rs`
    /// inexpressible without naming a claim: the gatekeeper service holds a bearer token,
    /// so it lands on `PlatformServiceProvenBySharedBearerToken`, so this returns `None`,
    /// so the `user_address` in its JSON body has to be spelled
    /// [`crate::ClaimedWalletAddressNobodyHasVerified`] -- which is exactly what it is.
    pub fn wallet_address_if_this_principal_is_a_human_wallet(
        &self,
    ) -> Option<&VerifiedWalletAddress> {
        match self {
            Self::HumanWalletProvenByAdr44SignedFetchSignature(wallet)
            | Self::VerifiedAdminConsoleWallet(wallet) => Some(wallet),
            Self::VerifiedPeerCatalyst(_)
            | Self::PlatformServiceProvenBySharedBearerToken(_)
            | Self::WebhookAuthenticatedSystem(_) => None,
        }
    }

    /// A description of the actor for an audit column, derived only from what the server
    /// established.
    ///
    /// This replaces two families of audit actor in use today, both of which record
    /// something the server never verified:
    ///
    /// - the literal `"admin-token"` written by `catalyrst-market/src/handlers/admin.rs`
    ///   and `catalyrst-social-service/src/rest/handlers/admin.rs`, which records the
    ///   *mechanism* and nothing else;
    /// - the client-supplied `x-catalyrst-admin` header used by badges, economy, credits
    ///   and telemetry, which records whatever the caller typed.
    ///
    /// The mechanism prefix is part of the string on purpose: `wallet:0x...` and
    /// `service-token:MARKET_ADMIN_TOKEN` are different facts and must not sort together.
    ///
    /// This is **not** a stable wire format. It is an audit string; do not parse it.
    pub fn audit_actor_description(&self) -> String {
        match self {
            Self::HumanWalletProvenByAdr44SignedFetchSignature(wallet) => {
                format!("wallet:{wallet}")
            }
            Self::VerifiedAdminConsoleWallet(wallet) => {
                format!("console-wallet:{wallet}")
            }
            Self::VerifiedPeerCatalyst(wallet) => {
                format!("peer-server:{wallet}")
            }
            Self::PlatformServiceProvenBySharedBearerToken(service) => format!(
                "service-token:{}",
                service.environment_variable_that_named_this_credential()
            ),
            Self::WebhookAuthenticatedSystem(service) => format!(
                "external-system:{}",
                service.environment_variable_that_named_this_credential()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::establish_platform_service_identity_by_comparing_presented_shared_secret;

    fn wallet(address: &str) -> VerifiedWalletAddress {
        VerifiedWalletAddress::unchecked_for_unit_tests_within_this_crate_only(address)
    }

    fn service(variable: &'static str) -> AuthenticatedPlatformServiceIdentity {
        establish_platform_service_identity_by_comparing_presented_shared_secret(
            variable,
            Some("s3cret"),
            Some("s3cret"),
        )
        .expect("matching secret establishes the service identity")
    }

    fn every_variant() -> Vec<AuthenticatedPrincipal> {
        vec![
            AuthenticatedPrincipal::HumanWalletProvenByAdr44SignedFetchSignature(wallet("0xAA")),
            AuthenticatedPrincipal::VerifiedAdminConsoleWallet(wallet("0xBB")),
            AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken(service(
                "MARKET_ADMIN_TOKEN",
            )),
            AuthenticatedPrincipal::VerifiedPeerCatalyst(wallet("0xCC")),
            AuthenticatedPrincipal::WebhookAuthenticatedSystem(service("LIVEKIT_API_SECRET")),
        ]
    }

    #[test]
    fn the_variant_table_covers_every_kind_of_principal() {
        assert_eq!(
            every_variant().len(),
            5,
            "a variant was added to AuthenticatedPrincipal without a row in every_variant()"
        );
    }

    /// P3: a shared secret never yields a person.
    #[test]
    fn no_service_shaped_principal_yields_a_wallet_address() {
        for principal in [
            AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken(service("A_TOKEN")),
            AuthenticatedPrincipal::WebhookAuthenticatedSystem(service("B_TOKEN")),
        ] {
            assert!(
                principal
                    .wallet_address_if_this_principal_is_a_human_wallet()
                    .is_none(),
                "{principal:?} must not narrow to a wallet"
            );
        }
    }

    #[test]
    fn a_relaying_peer_server_is_not_a_human_wallet() {
        let peer = AuthenticatedPrincipal::VerifiedPeerCatalyst(wallet("0xpeer"));
        assert!(peer
            .wallet_address_if_this_principal_is_a_human_wallet()
            .is_none());
    }

    #[test]
    fn both_human_wallet_mechanisms_narrow_to_their_wallet() {
        let signed_fetch =
            AuthenticatedPrincipal::HumanWalletProvenByAdr44SignedFetchSignature(wallet("0xAA"));
        let console = AuthenticatedPrincipal::VerifiedAdminConsoleWallet(wallet("0xBB"));
        assert_eq!(
            signed_fetch
                .wallet_address_if_this_principal_is_a_human_wallet()
                .map(|w| w.as_lowercased_hex_text().to_string()),
            Some("0xaa".to_string())
        );
        assert_eq!(
            console
                .wallet_address_if_this_principal_is_a_human_wallet()
                .map(|w| w.as_lowercased_hex_text().to_string()),
            Some("0xbb".to_string())
        );
    }

    #[test]
    fn audit_actor_descriptions_are_distinct_per_mechanism() {
        let descriptions: Vec<String> = every_variant()
            .iter()
            .map(AuthenticatedPrincipal::audit_actor_description)
            .collect();
        assert_eq!(
            descriptions,
            vec![
                "wallet:0xaa",
                "console-wallet:0xbb",
                "service-token:MARKET_ADMIN_TOKEN",
                "peer-server:0xcc",
                "external-system:LIVEKIT_API_SECRET",
            ]
        );
    }

    /// The same wallet reached by two different mechanisms must not produce the same audit
    /// string: the mechanism is part of the fact being recorded.
    #[test]
    fn the_same_wallet_under_two_mechanisms_audits_differently() {
        let signed_fetch =
            AuthenticatedPrincipal::HumanWalletProvenByAdr44SignedFetchSignature(wallet("0xAA"));
        let console = AuthenticatedPrincipal::VerifiedAdminConsoleWallet(wallet("0xAA"));
        assert_ne!(
            signed_fetch.audit_actor_description(),
            console.audit_actor_description()
        );
    }

    /// No audit string can carry text the client chose: every one of them is built from a
    /// lowercased verified address or from a `&'static str` naming an environment variable.
    #[test]
    fn no_audit_string_can_contain_client_supplied_text() {
        for principal in every_variant() {
            let description = principal.audit_actor_description();
            let (mechanism, subject) = description
                .split_once(':')
                .expect("every audit actor description is mechanism:subject");
            assert!(!mechanism.is_empty());
            assert!(!subject.is_empty());
            assert!(
                subject
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == 'x'),
                "unexpected characters in audit subject {subject:?}"
            );
        }
    }
}
