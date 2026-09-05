use crate::refusal::AuthorityNotEstablished;

/// A **service**, established by possession of a shared static secret.
///
/// # What a value of this type proves
///
/// That the caller presented a byte string equal to the secret configured under one named
/// environment variable. That is all a shared secret can ever prove, and it is a fact about
/// a *deployment*, not about a person.
///
/// # What it does NOT prove
///
/// - **Not a person.** There is no wallet address inside this type and there is no function
///   anywhere that produces one from it. Any human name that travels alongside a shared
///   secret -- `?moderator=`, `x-catalyrst-admin` -- is attacker-chosen: see
///   [`crate::UnverifiedOperatorDisplayName`] and
///   [`crate::UnverifiedAdminDisplayName`], which are audit-column
///   types and cannot be compared to an allowlist.
/// - **Not which instance, which host, or which request.** A shared secret is copied to
///   every replica of every caller that holds it. If it leaks, this type is satisfied by
///   whoever holds the leak.
/// - **Not authorization.** It says a service called, never that the service may act.
///
/// # How a value is obtained
///
/// Exactly one way:
/// [`establish_platform_service_identity_by_comparing_presented_shared_secret`]. The
/// constructor itself is crate-private, so unforgeability here is *real* privacy rather
/// than something inherited from another crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthenticatedPlatformServiceIdentity {
    environment_variable_that_named_this_credential: &'static str,
}

impl AuthenticatedPlatformServiceIdentity {
    /// Crate-private. The only public way in is
    /// [`establish_platform_service_identity_by_comparing_presented_shared_secret`].
    pub(crate) fn established_by_a_matching_shared_secret(
        environment_variable_that_named_this_credential: &'static str,
    ) -> Self {
        Self {
            environment_variable_that_named_this_credential,
        }
    }

    /// The only honest audit identity a shared secret can yield: **which secret**.
    ///
    /// This replaces the literal `"admin-token"` actor string written into audit rows by
    /// `catalyrst-market/src/handlers/admin.rs` and
    /// `catalyrst-social-service/src/rest/handlers/admin.rs`. It is a name the server
    /// configured, not one the client chose.
    pub fn environment_variable_that_named_this_credential(&self) -> &'static str {
        self.environment_variable_that_named_this_credential
    }
}

/// The only mint for [`AuthenticatedPlatformServiceIdentity`]: compare a presented shared
/// secret against the configured one in constant time.
///
/// # Why this exists rather than a `pub fn new`
///
/// A `pub fn new(&'static str)` would let any caller assert a service identity it never
/// verified -- the same hole the wallet chokepoint closes. Making the constructor
/// crate-private and exposing only this comparison means the type cannot exist unless the
/// secret matched, and the match happened here, once, with the same semantics for all
/// twenty-one bearer gates that will eventually use it.
///
/// # The three refusals, and why they are three
///
/// - `configured_secret` is `None` or empty =>
///   [`AuthorityNotEstablished::CredentialNotConfigured`] => **503**. The deployment is
///   broken, not the caller. This generalizes `catalyrst-comms/src/moderator.rs`'s
///   `require_service_token`, the only gate in the workspace that already does this.
///   Treating an empty configured secret as unconfigured is deliberate and is **stricter**
///   than several existing gates, which would happily accept an empty presented secret
///   against an empty configured one; a migration that adopts this function should say so.
/// - `presented_secret` is `None` => [`AuthorityNotEstablished::AuthenticationMissingOrInvalid`]
///   => **401**.
/// - the secrets differ => [`AuthorityNotEstablished::PresentedSharedSecretDidNotMatch`]
///   => **401**.
///
/// # What it does not do
///
/// It does not parse the `Authorization` header. Header parsing stays with the caller
/// because the workspace does not agree on it -- twenty of twenty-one gates require the
/// exact prefix `"Bearer "`, and `catalyrst-places/src/auth.rs` also accepts lowercase
/// `"bearer "`. Widening the shared gate to match places would loosen twenty gates at
/// once; places is the one that should change. Pass the already-extracted token here.
///
/// # Constant time, with the usual caveat
///
/// The comparison is constant time in the *contents* of two equal-length secrets. A length
/// difference short-circuits, which leaks the length -- identical to the `timing_safe_eq`
/// helpers already deployed across the workspace, and not a regression.
pub fn establish_platform_service_identity_by_comparing_presented_shared_secret(
    environment_variable_that_named_this_credential: &'static str,
    configured_secret: Option<&str>,
    presented_secret: Option<&str>,
) -> Result<AuthenticatedPlatformServiceIdentity, AuthorityNotEstablished> {
    let configured = match configured_secret {
        Some(secret) if !secret.is_empty() => secret,
        _ => {
            return Err(AuthorityNotEstablished::CredentialNotConfigured {
                environment_variable: environment_variable_that_named_this_credential,
            })
        }
    };

    let Some(presented) = presented_secret else {
        return Err(AuthorityNotEstablished::AuthenticationMissingOrInvalid {
            detail: "no shared secret presented",
        });
    };

    if constant_time_bytes_are_equal(presented.as_bytes(), configured.as_bytes()) {
        Ok(
            AuthenticatedPlatformServiceIdentity::established_by_a_matching_shared_secret(
                environment_variable_that_named_this_credential,
            ),
        )
    } else {
        Err(AuthorityNotEstablished::PresentedSharedSecretDidNotMatch)
    }
}

/// Byte-for-byte comparison that does not short-circuit on the first differing byte.
/// Copied in behaviour from the `timing_safe_eq` helpers already in
/// `catalyrst-comms/src/moderator.rs` and `catalyrst-badges/src/admin.rs`, so migrating a
/// gate onto this function changes no timing property it had before.
fn constant_time_bytes_are_equal(presented: &[u8], configured: &[u8]) -> bool {
    if presented.len() != configured.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in presented.iter().zip(configured.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARIABLE: &str = "COMMS_GATEKEEPER_AUTH_TOKEN";

    #[test]
    fn a_matching_secret_yields_an_identity_that_names_only_the_variable() {
        let identity = establish_platform_service_identity_by_comparing_presented_shared_secret(
            VARIABLE,
            Some("s3cret"),
            Some("s3cret"),
        )
        .expect("matching secret establishes the service identity");
        assert_eq!(
            identity.environment_variable_that_named_this_credential(),
            VARIABLE
        );
    }

    #[test]
    fn an_unconfigured_credential_is_a_misconfiguration_and_never_a_denial() {
        let refusal = establish_platform_service_identity_by_comparing_presented_shared_secret(
            VARIABLE,
            None,
            Some("anything"),
        )
        .expect_err("an unconfigured credential cannot establish an identity");
        assert!(matches!(
            refusal,
            AuthorityNotEstablished::CredentialNotConfigured {
                environment_variable: VARIABLE
            }
        ));
        assert_eq!(refusal.http_status(), 503);
    }

    #[test]
    fn an_empty_configured_secret_counts_as_unconfigured() {
        let refusal = establish_platform_service_identity_by_comparing_presented_shared_secret(
            VARIABLE,
            Some(""),
            Some(""),
        )
        .expect_err("an empty configured secret must not authenticate anyone");
        assert_eq!(refusal.http_status(), 503);
    }

    #[test]
    fn presenting_nothing_is_a_401_distinct_from_presenting_the_wrong_thing() {
        let nothing = establish_platform_service_identity_by_comparing_presented_shared_secret(
            VARIABLE,
            Some("s3cret"),
            None,
        )
        .expect_err("no presented secret cannot establish an identity");
        assert!(matches!(
            nothing,
            AuthorityNotEstablished::AuthenticationMissingOrInvalid { .. }
        ));

        let wrong = establish_platform_service_identity_by_comparing_presented_shared_secret(
            VARIABLE,
            Some("s3cret"),
            Some("wrong"),
        )
        .expect_err("a mismatched secret cannot establish an identity");
        assert!(matches!(
            wrong,
            AuthorityNotEstablished::PresentedSharedSecretDidNotMatch
        ));

        assert_eq!(nothing.http_status(), 401);
        assert_eq!(wrong.http_status(), 401);
    }

    #[test]
    fn a_prefix_of_the_secret_does_not_match() {
        let refusal = establish_platform_service_identity_by_comparing_presented_shared_secret(
            VARIABLE,
            Some("s3cret"),
            Some("s3cr"),
        )
        .expect_err("a prefix is not the secret");
        assert!(matches!(
            refusal,
            AuthorityNotEstablished::PresentedSharedSecretDidNotMatch
        ));
    }

    #[test]
    fn the_comparison_is_case_sensitive_and_whitespace_sensitive() {
        for presented in ["S3CRET", " s3cret", "s3cret "] {
            let refusal = establish_platform_service_identity_by_comparing_presented_shared_secret(
                VARIABLE,
                Some("s3cret"),
                Some(presented),
            )
            .expect_err("near misses are misses");
            assert!(matches!(
                refusal,
                AuthorityNotEstablished::PresentedSharedSecretDidNotMatch
            ));
        }
    }

    #[test]
    fn constant_time_comparison_agrees_with_equality() {
        assert!(constant_time_bytes_are_equal(b"abc", b"abc"));
        assert!(!constant_time_bytes_are_equal(b"abc", b"abd"));
        assert!(!constant_time_bytes_are_equal(b"abc", b"ab"));
        assert!(constant_time_bytes_are_equal(b"", b""));
    }
}
