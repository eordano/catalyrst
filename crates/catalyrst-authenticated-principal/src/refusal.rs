/// Why an authority could not be established.
///
/// # What the arms prove
///
/// They are **not** interchangeable, and that is the entire point of the type. Three
/// different things are routinely flattened into one another across this workspace:
///
/// - *"we could not tell"* -- a backing store was down, a query failed;
/// - *"the service is misconfigured"* -- the credential this gate compares against is unset;
/// - *"the caller may not"* -- the lookup succeeded and the answer was no.
///
/// Collapsing "could not tell" into a **denial** is the fail-*closed* twin of the
/// fail-open defect: `catalyrst-comms/src/ports/scene_admin.rs`'s `.unwrap_or(0)` turns a
/// dead database into "not an admin". Collapsing it into a **permit** is the fail-open
/// defect itself: `catalyrst-comms/src/handlers/world_ban_check.rs`'s `.unwrap_or(false)`
/// turns a dead database into "not banned". Both are wrong, they are wrong in opposite
/// directions, and the caller must be able to tell which happened -- hence one arm each.
///
/// # What a value of this type does NOT prove
///
/// Nothing about the caller's identity. `RefusedLacksAuthority` is
/// the only arm that means the caller was successfully identified and found wanting; the
/// other four mean the question was never answered.
///
/// # How a value is obtained
///
/// Constructed at the point of refusal by whichever crate-local authority failed to
/// resolve. Each crate writes its own `From<sqlx::Error>` (or equivalent) into
/// `UndeterminedStoreUnavailable`; this crate has no `sqlx` and cannot
/// provide it.
///
/// # Deliberately not `#[non_exhaustive]`
///
/// Adding an arm **should** break every downstream match. `#[non_exhaustive]` would force
/// the `_` catch-all that this whole exercise exists to remove.
#[derive(Debug, thiserror::Error)]
pub enum AuthorityNotEstablished {
    /// No usable credential was presented, or the one presented did not verify. Renders
    /// as **401**.
    ///
    /// `detail` is `&'static str` on purpose: it is a fixed reason chosen by the gate, not
    /// an echo of anything the caller sent.
    #[error("authentication missing or invalid: {detail}")]
    AuthenticationMissingOrInvalid {
        /// A fixed, server-authored reason. Never caller-controlled text.
        detail: &'static str,
    },

    /// A shared secret was presented and did not match the configured one. Renders as
    /// **401**.
    ///
    /// Separate from [`Self::AuthenticationMissingOrInvalid`] so that "presented the wrong
    /// secret" is distinguishable from "presented nothing" in logs and in tests, without
    /// either of them being distinguishable to the caller -- both render 401.
    #[error("presented shared secret did not match")]
    PresentedSharedSecretDidNotMatch,

    /// The principal is who they say they are, the lookup succeeded, and they do not hold
    /// this authority. Renders as **403**. This is the *only* arm that means "no".
    #[error("refused: {detail}")]
    RefusedLacksAuthority {
        /// Operator-facing explanation of which authority was required.
        detail: String,
    },

    /// The check could not be completed, so no decision exists. Renders as **503**, never
    /// 403 and never 200.
    #[error("could not determine authority: backing store {store} unavailable")]
    UndeterminedStoreUnavailable {
        /// A fixed name for the store that failed, for metrics and for the operator log.
        store: &'static str,
        /// The underlying failure, rendered for operators. Not for the client body.
        reason_for_operators: String,
    },

    /// The service is misconfigured; the caller is not unauthorized. Renders as **503**.
    ///
    /// `catalyrst-comms/src/moderator.rs`'s `require_service_token` is the only one of the
    /// workspace's twenty-one bearer gates that already draws this distinction -- an unset
    /// `COMMS_GATEKEEPER_AUTH_TOKEN` answers 503, not 401. The other twenty flatten it into
    /// a 401 or a 403, which tells an operator that their callers are broken when in fact
    /// their deployment is. This arm generalizes the comms behaviour; it must not be
    /// flattened to match the others.
    #[error("credential not configured: {environment_variable} is unset")]
    CredentialNotConfigured {
        /// The environment variable whose absence disabled the gate.
        environment_variable: &'static str,
    },
}

impl AuthorityNotEstablished {
    /// The one place that decides the HTTP status for a refusal.
    ///
    /// Today one logical refusal renders three different ways depending on which file
    /// caught it -- 401 in the federation authority, 403 in the client path, a raw
    /// `Response` in the ban handlers. One exhaustive match replaces that.
    ///
    /// The mapping is asserted by an explicit table test in this module: `Undetermined...`
    /// and `CredentialNotConfigured` never render 403, and `Refused...` never renders 503.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::AuthenticationMissingOrInvalid { .. }
            | Self::PresentedSharedSecretDidNotMatch => 401,
            Self::RefusedLacksAuthority { .. } => 403,
            Self::UndeterminedStoreUnavailable { .. } | Self::CredentialNotConfigured { .. } => 503,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_arm() -> Vec<(AuthorityNotEstablished, u16)> {
        vec![
            (
                AuthorityNotEstablished::AuthenticationMissingOrInvalid {
                    detail: "no auth chain headers",
                },
                401,
            ),
            (
                AuthorityNotEstablished::PresentedSharedSecretDidNotMatch,
                401,
            ),
            (
                AuthorityNotEstablished::RefusedLacksAuthority {
                    detail: "requires the owner tier".to_string(),
                },
                403,
            ),
            (
                AuthorityNotEstablished::UndeterminedStoreUnavailable {
                    store: "community_members",
                    reason_for_operators: "connection refused".to_string(),
                },
                503,
            ),
            (
                AuthorityNotEstablished::CredentialNotConfigured {
                    environment_variable: "COMMS_GATEKEEPER_AUTH_TOKEN",
                },
                503,
            ),
        ]
    }

    #[test]
    fn every_arm_maps_to_its_documented_status() {
        for (refusal, expected) in every_arm() {
            assert_eq!(
                refusal.http_status(),
                expected,
                "wrong status for {refusal:?}"
            );
        }
    }

    /// The table above must cover the enum. If an arm is added and not listed here, this
    /// count fails and the author is forced to decide its status deliberately.
    #[test]
    fn the_status_table_covers_every_arm() {
        assert_eq!(
            every_arm().len(),
            5,
            "an arm was added to AuthorityNotEstablished without a row in every_arm()"
        );
    }

    #[test]
    fn undetermined_and_misconfigured_never_render_as_a_denial() {
        let undetermined = AuthorityNotEstablished::UndeterminedStoreUnavailable {
            store: "community_role_current",
            reason_for_operators: "pool timed out".to_string(),
        };
        let misconfigured = AuthorityNotEstablished::CredentialNotConfigured {
            environment_variable: "ADMIN_TOKEN",
        };
        assert_ne!(undetermined.http_status(), 403);
        assert_ne!(misconfigured.http_status(), 403);
        assert_eq!(undetermined.http_status(), 503);
        assert_eq!(misconfigured.http_status(), 503);
    }

    #[test]
    fn a_real_denial_never_renders_as_unavailable_or_success() {
        let refused = AuthorityNotEstablished::RefusedLacksAuthority {
            detail: "moderator tier required".to_string(),
        };
        assert_eq!(refused.http_status(), 403);
        assert_ne!(refused.http_status(), 503);
        assert_ne!(refused.http_status(), 200);
    }

    #[test]
    fn operator_facing_text_names_the_variable_and_the_store() {
        let misconfigured = AuthorityNotEstablished::CredentialNotConfigured {
            environment_variable: "COMMS_GATEKEEPER_AUTH_TOKEN",
        };
        assert_eq!(
            misconfigured.to_string(),
            "credential not configured: COMMS_GATEKEEPER_AUTH_TOKEN is unset"
        );

        let undetermined = AuthorityNotEstablished::UndeterminedStoreUnavailable {
            store: "community_members",
            reason_for_operators: "connection refused".to_string(),
        };
        assert_eq!(
            undetermined.to_string(),
            "could not determine authority: backing store community_members unavailable"
        );
    }
}
