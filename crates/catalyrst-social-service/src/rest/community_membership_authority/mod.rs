//! Community membership standings and the moderation authorities derived from them.
//!
//! This module is the single typed path that replaces the five separate role
//! implementations this crate used to carry. It is deliberately crate-local: nothing here
//! is re-exported past `catalyrst-social-service`, because "may X do Y to Z" is a
//! per-domain question and `docs/authz-confusion-defense.md` is explicit that scope types
//! stay crate-local. Only the *principal* vocabulary is shared, and that lives in
//! `catalyrst-authenticated-principal`.
//!
//! # What is here
//!
//! - [`standing`] -- the tier, the table it came from, and the one parse of a stored role
//!   string.
//! - [`ban_authority`] -- the community-scoped ban and unban authorities, with one shared
//!   predicate behind three constructors each.
//!
//! # Rendering a refusal
//!
//! [`AuthorityNotEstablished`] draws a distinction the old code could not: *we could not
//! tell* is not *no*. [`status_and_message_for_refusal`] is the one place this crate turns
//! that distinction into an HTTP status.

pub mod ban_authority;
pub mod standing;

use axum::http::StatusCode;
use catalyrst_authenticated_principal::AuthorityNotEstablished;

pub use ban_authority::{CommunityBanAuthority, CommunityUnbanAuthority};
pub use standing::{
    load_standing_from_community_members, load_standing_from_community_role_current,
    CommunityMembershipStanding, CommunityMembershipTier, CommunityMembershipTierSourceTable,
};

/// Turn a refusal into the status and body message one call site should answer with.
///
/// # Why the caller supplies the refusal status and message
///
/// The same logical refusal renders three different ways in this crate today: 401 from
/// the client write paths, 403 from the federation write paths, and a bare string from
/// the gossip consumer. Unifying *that* is a client-visible change to a shipped API and
/// is deliberately **not** part of this migration; each call site keeps the status and the
/// wording it has always sent. What is unified is the thing that mattered: which of the
/// five arms was reached, and the guarantee that an outage never reaches the refusal arm.
///
/// # Why `Undetermined` renders 500 here and not the 503 `http_status()` reports
///
/// Both paths answer 500 for a database fault today --
/// `writes::mod::map_apply_err`'s `ApiError::Database` arm and
/// `client::mod::map_db` both do. `AuthorityNotEstablished::http_status()` argues for 503
/// and is right, but changing it is a separate, client-visible commit. The property that
/// matters is preserved either way: a backing-store failure is not a 403, so it can never
/// be mistaken for a decision.
pub fn status_and_message_for_refusal(
    refusal: &AuthorityNotEstablished,
    status_when_the_principal_lacks_the_authority: StatusCode,
    message_when_the_principal_lacks_the_authority: impl FnOnce() -> String,
) -> (StatusCode, String) {
    match refusal {
        AuthorityNotEstablished::RefusedLacksAuthority { detail } => {
            tracing::debug!(%detail, "community membership authority refused");
            (
                status_when_the_principal_lacks_the_authority,
                message_when_the_principal_lacks_the_authority(),
            )
        }
        AuthorityNotEstablished::AuthenticationMissingOrInvalid { detail } => {
            (StatusCode::UNAUTHORIZED, format!("auth chain: {detail}"))
        }
        AuthorityNotEstablished::PresentedSharedSecretDidNotMatch => (
            StatusCode::UNAUTHORIZED,
            "presented shared secret did not match".to_string(),
        ),
        AuthorityNotEstablished::UndeterminedStoreUnavailable {
            store,
            reason_for_operators,
        } => {
            tracing::error!(
                %store,
                reason = %reason_for_operators,
                "community membership authority could not be determined; refusing without deciding"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error".to_string(),
            )
        }
        AuthorityNotEstablished::CredentialNotConfigured {
            environment_variable,
        } => {
            tracing::error!(
                %environment_variable,
                "community membership authority reached a credential gate it does not use"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "credential not configured".to_string(),
            )
        }
    }
}

/// [`status_and_message_for_refusal`] for the call sites whose refusal message was
/// already the authority's own explanation rather than a site-specific sentence -- the
/// federation write gates, whose `"Forbidden: signer role X below required Y"` text is
/// produced by the authority itself.
pub fn status_and_message_for_refusal_using_its_own_detail(
    refusal: &AuthorityNotEstablished,
    status_when_the_principal_lacks_the_authority: StatusCode,
) -> (StatusCode, String) {
    let detail = match refusal {
        AuthorityNotEstablished::RefusedLacksAuthority { detail } => detail.clone(),
        _ => String::new(),
    };
    status_and_message_for_refusal(
        refusal,
        status_when_the_principal_lacks_the_authority,
        move || detail,
    )
}

/// The gossip consumer has no HTTP status to answer with -- `apply_envelope` returns
/// `Result<(), String>` and the string is logged, not sent. This renders a refusal for
/// that channel without inventing a status for it.
pub fn operator_log_message_for_refusal(refusal: &AuthorityNotEstablished) -> String {
    refusal.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_keeps_the_call_sites_own_status_and_wording() {
        let refusal = AuthorityNotEstablished::RefusedLacksAuthority {
            detail: "0xactor holds the member tier".to_string(),
        };
        let (status, message) =
            status_and_message_for_refusal(&refusal, StatusCode::UNAUTHORIZED, || {
                "The user 0xactor doesn't have permission to ban 0xtarget".to_string()
            });
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            message,
            "The user 0xactor doesn't have permission to ban 0xtarget"
        );

        let (status, _) = status_and_message_for_refusal(&refusal, StatusCode::FORBIDDEN, || {
            "forbidden".to_string()
        });
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn an_undetermined_authority_never_renders_as_the_call_sites_refusal() {
        let undetermined = AuthorityNotEstablished::UndeterminedStoreUnavailable {
            store: "community_members",
            reason_for_operators: "pool timed out".to_string(),
        };
        for call_site_refusal_status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            let (status, message) =
                status_and_message_for_refusal(&undetermined, call_site_refusal_status, || {
                    unreachable!("the refusal message must not be built for an outage")
                });
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(message, "database error");
            assert_ne!(status, call_site_refusal_status);
        }
    }

    #[test]
    fn the_operator_log_message_names_the_store_that_failed() {
        let undetermined = AuthorityNotEstablished::UndeterminedStoreUnavailable {
            store: "community_role_current",
            reason_for_operators: "connection refused".to_string(),
        };
        assert!(operator_log_message_for_refusal(&undetermined).contains("community_role_current"));
    }
}
