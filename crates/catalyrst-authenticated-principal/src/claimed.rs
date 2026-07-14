//! Claims, as first-class types.
//!
//! Everything in this file is something a caller **told us**. Nothing in this file has any
//! conversion into [`crate::VerifiedWalletAddress`] or into any membership
//! standing, and none of these types has `AsRef<str>` or a `PartialEq` against a verified
//! type. Turning a claim into an identity requires a signature check; turning a claim into
//! a standing requires a database round-trip. **The absence of a `From` impl is how those
//! requirements are expressed.**
//!
//! This is also where the crate's only `Deserialize` derives live, and
//! `tests/source_discipline.rs` asserts that they live nowhere else.

/// Maximum length of a sanitized operator display name, in bytes.
///
/// Byte-identical to `MAX_MODERATOR_NAME_LENGTH` in `catalyrst-comms/src/moderator.rs`,
/// from which [`UnverifiedOperatorDisplayName::sanitize`] is lifted.
const MAXIMUM_OPERATOR_DISPLAY_NAME_LENGTH: usize = 100;

/// Maximum number of characters kept from a client-supplied administrator display name.
///
/// Characters, not bytes -- matching `catalyrst-badges/src/admin.rs`'s
/// `.chars().take(100)`, which this type reproduces exactly.
const MAXIMUM_ADMINISTRATOR_DISPLAY_NAME_CHARACTERS: usize = 100;

/// A wallet address a caller **told us about**.
///
/// # What a value of this type proves
///
/// Nothing whatsoever. It is a string that arrived in a request body or a query string.
///
/// # What it does NOT prove
///
/// That anybody controls this address; that it is well-formed; that the caller has any
/// relationship to it at all. This is the type of `CommunityVoiceChatBody.user_address` in
/// `catalyrst-comms/src/handlers/voice.rs`, where a caller holding
/// `COMMS_GATEKEEPER_AUTH_TOKEN` may name **any** wallet. That trust boundary is
/// deliberate, documented in that file, and pinned by
/// `catalyrst-comms/tests/voice_auth_fail_closed.rs`; this type documents it in the type
/// system rather than changing it.
///
/// # How a value is obtained
///
/// Deserialized from a request, or built from untrusted text with
/// [`Self::from_untrusted_text`]. Both are safe, because the value asserts nothing.
///
/// # Deliberately absent
///
/// `From<ClaimedWalletAddressNobodyHasVerified> for VerifiedWalletAddress`,
/// `TryFrom`, `AsRef<str>`, and any `PartialEq` against a verified type. There is one exit,
/// [`Self::as_unverified_text`], named so it is visible at every use site.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(transparent)]
pub struct ClaimedWalletAddressNobodyHasVerified(String);

impl ClaimedWalletAddressNobodyHasVerified {
    /// Wrap untrusted text. Safe by construction: the result proves nothing, so there is
    /// nothing to forge.
    pub fn from_untrusted_text(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The only exit. Named at length so a reviewer sees, at the use site, that what is
    /// leaving the type has not been verified by anything.
    pub fn as_unverified_text(&self) -> &str {
        &self.0
    }
}

/// A community role name a caller **told us about** -- `user_role` in
/// `catalyrst-comms/src/handlers/voice.rs`.
///
/// # What a value of this type proves
///
/// Nothing. A service holding a bearer token asserted that some wallet holds some role.
///
/// # What it does NOT prove
///
/// That the named wallet holds that role, that the role name is one of the tiers the
/// database can store, or that any community was consulted. The comms voice path decides
/// LiveKit publish rights from this claim and does not look in a database; that is the
/// documented behaviour of that path and this type does not change it.
///
/// # How a value is obtained
///
/// Deserialized from a request body, or [`Self::from_untrusted_text`].
///
/// # Deliberately absent
///
/// Any conversion into a membership standing. That requires reading the row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(transparent)]
pub struct ClaimedCommunityRoleNameNobodyHasVerified(String);

impl ClaimedCommunityRoleNameNobodyHasVerified {
    /// Wrap untrusted text. Safe by construction: the result proves nothing.
    pub fn from_untrusted_text(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The only exit.
    pub fn as_unverified_text(&self) -> &str {
        &self.0
    }
}

/// The human name a token-holding caller put in the `?moderator=` query parameter
/// (`catalyrst-comms/src/moderator.rs`).
///
/// # What a value of this type proves
///
/// Only that the text is non-empty, at most 100 bytes, and drawn from
/// `[A-Za-z0-9 _.-]`. It has been *shape-checked*, never *authenticated*.
///
/// # What it does NOT prove
///
/// That such an operator exists, that they authorized anything, or that the token holder
/// is that person. Whoever holds the moderation service token may write any name here.
///
/// # How a value is obtained
///
/// [`Self::sanitize`], which is the charset and length check lifted verbatim from
/// `sanitize_moderator_name` in `catalyrst-comms/src/moderator.rs`.
///
/// # Deliberately absent
///
/// `as_str`. The only exit is [`Self::into_audit_column_value`], which consumes the value,
/// so this name cannot be compared against an allowlist or against a verified address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedOperatorDisplayName(String);

impl UnverifiedOperatorDisplayName {
    /// Trim, then accept only a non-empty name of at most 100 bytes drawn from ASCII
    /// alphanumerics, space, `_`, `-` and `.`.
    ///
    /// Behaviour-identical to `catalyrst-comms/src/moderator.rs`'s
    /// `sanitize_moderator_name`, including the fact that the length bound is applied to
    /// the trimmed byte length, not the character count.
    pub fn sanitize(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > MAXIMUM_OPERATOR_DISPLAY_NAME_LENGTH {
            return None;
        }
        let charset_is_acceptable = trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-' || c == '.');
        if charset_is_acceptable {
            Some(Self(trimmed.to_string()))
        } else {
            None
        }
    }

    /// The only exit, and it consumes the value: this text is fit for an audit column and
    /// for nothing else.
    pub fn into_audit_column_value(self) -> String {
        self.0
    }
}

/// The human name a client put in the `x-catalyrst-admin` header
/// (`catalyrst-badges/src/admin.rs`, and the same header in economy, credits and
/// telemetry).
///
/// # What a value of this type proves
///
/// Only that the text is non-empty after trimming. It is attacker-chosen.
///
/// # What it does NOT prove
///
/// Anything about who acted. The bearer token that accompanies this header authenticates a
/// *service*; this header names a person nobody checked. The replacement for an audit row
/// that must record something real is
/// [`crate::AuthenticatedPrincipal::audit_actor_description`].
///
/// # How a value is obtained
///
/// [`Self::sanitize`].
///
/// # Deliberately absent
///
/// `as_str`, and the `unwrap_or("admin-token")` fallback that `catalyrst-badges`' current
/// `admin_actor` applies. Substituting a literal when the header is missing is a decision
/// for the migrating caller to make explicitly, not a silent property of this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedAdminDisplayName(String);

impl UnverifiedAdminDisplayName {
    /// Trim, reject empty, and keep at most the first 100 **characters**.
    ///
    /// Behaviour-identical to the accepted half of `catalyrst-badges/src/admin.rs`'s
    /// `admin_actor` -- `.map(str::trim).filter(|s| !s.is_empty()).map(|s|
    /// s.chars().take(100).collect())` -- minus its fallback, which is the caller's to make.
    pub fn sanitize(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(Self(
            trimmed
                .chars()
                .take(MAXIMUM_ADMINISTRATOR_DISPLAY_NAME_CHARACTERS)
                .collect(),
        ))
    }

    /// The only exit, and it consumes the value.
    pub fn into_audit_column_value(self) -> String {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claimed_wallet_address_deserializes_transparently_from_a_bare_string() {
        let claimed: ClaimedWalletAddressNobodyHasVerified =
            serde_json::from_str("\"0xDEADBEEF\"").expect("transparent newtype over a string");
        // Deliberately NOT normalized: a claim is recorded as it arrived, so an audit row
        // shows what the caller actually sent.
        assert_eq!(claimed.as_unverified_text(), "0xDEADBEEF");
    }

    #[test]
    fn a_claimed_role_name_deserializes_transparently_from_a_bare_string() {
        let claimed: ClaimedCommunityRoleNameNobodyHasVerified =
            serde_json::from_str("\"moderator\"").expect("transparent newtype over a string");
        assert_eq!(claimed.as_unverified_text(), "moderator");
    }

    #[test]
    fn a_claimed_role_name_accepts_text_no_database_would_ever_hold() {
        // The point of the type: nothing constrains a claim to the tier vocabulary.
        let claimed = ClaimedCommunityRoleNameNobodyHasVerified::from_untrusted_text(
            "owner'; DROP TABLE community_members;--",
        );
        assert_eq!(
            claimed.as_unverified_text(),
            "owner'; DROP TABLE community_members;--"
        );
    }

    #[test]
    fn an_operator_display_name_keeps_the_comms_charset_rules() {
        for accepted in ["alice", "Alice Smith", "a_b-c.d", "0perator", "  padded  "] {
            assert!(
                UnverifiedOperatorDisplayName::sanitize(accepted).is_some(),
                "{accepted:?} should be accepted"
            );
        }
        for rejected in [
            "",
            "   ",
            "alice@example.com",
            "drop;table",
            "na\u{EF}ve",
            "a/b",
        ] {
            assert!(
                UnverifiedOperatorDisplayName::sanitize(rejected).is_none(),
                "{rejected:?} should be rejected"
            );
        }
    }

    #[test]
    fn an_operator_display_name_is_trimmed_and_length_bounded_at_one_hundred_bytes() {
        let padded =
            UnverifiedOperatorDisplayName::sanitize("  alice  ").expect("trims to a valid name");
        assert_eq!(padded.into_audit_column_value(), "alice");

        let exactly_one_hundred = "a".repeat(MAXIMUM_OPERATOR_DISPLAY_NAME_LENGTH);
        assert!(UnverifiedOperatorDisplayName::sanitize(&exactly_one_hundred).is_some());

        let one_too_many = "a".repeat(MAXIMUM_OPERATOR_DISPLAY_NAME_LENGTH + 1);
        assert!(UnverifiedOperatorDisplayName::sanitize(&one_too_many).is_none());
    }

    #[test]
    fn an_administrator_display_name_truncates_rather_than_rejecting() {
        let long = "b".repeat(MAXIMUM_ADMINISTRATOR_DISPLAY_NAME_CHARACTERS + 50);
        let sanitized = UnverifiedAdminDisplayName::sanitize(&long)
            .expect("badges truncates rather than rejecting");
        assert_eq!(
            sanitized.into_audit_column_value().chars().count(),
            MAXIMUM_ADMINISTRATOR_DISPLAY_NAME_CHARACTERS
        );
    }

    #[test]
    fn an_administrator_display_name_rejects_empty_and_whitespace_only() {
        assert!(UnverifiedAdminDisplayName::sanitize("").is_none());
        assert!(UnverifiedAdminDisplayName::sanitize("   ").is_none());
    }

    /// Unlike the comms operator name, the badges header has no charset rule today. This
    /// test pins that difference so nobody "fixes" one of the two into the other by
    /// accident: they are different fields on different paths with different histories.
    #[test]
    fn the_two_display_name_types_do_not_share_a_charset_rule() {
        let hostile = "alice@example.com";
        assert!(UnverifiedOperatorDisplayName::sanitize(hostile).is_none());
        assert!(UnverifiedAdminDisplayName::sanitize(hostile).is_some());
    }

    // Deliberately impossible, and each is a compile error today:
    //
    //   let verified: VerifiedWalletAddress = claimed.into();      // E0277
    //   VerifiedWalletAddress::try_from(claimed);                  // E0277
    //   claimed.as_unverified_text() == verified                                   // E0277
    //   allowlist.contains(&claimed)                                               // E0308
}
