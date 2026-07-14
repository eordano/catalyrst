//! The shared vocabulary for **who** a request is from, and **by what mechanism** that
//! was established.
//!
//! # What this crate is
//!
//! One *principal* vocabulary, shared workspace-wide. Every authentication mechanism in
//! the fleet answers the same question -- is this a human wallet, a platform service, a
//! peer catalyst server, or an external system -- and until now each answered it with a
//! bare `String` or an `Ok(())`. That distinction is what lives here.
//!
//! # What this crate deliberately is NOT
//!
//! It is **not** a permission model. There is no `Authorized<Scope, Capability>`, no
//! generic, no capability registry and no table of tenancy scopes; `docs/authz-confusion-
//! defense.md` rejects those and this crate does not reintroduce them. Every "may WHO do
//! WHAT to WHOM" type stays crate-local to the domain that owns the question, with its own
//! long explicit name. This crate shares only the *principal* and the *shape of a refusal*.
//!
//! It performs no I/O. It has no `sqlx`, no `ts-rs`, no `utoipa`, and no `Serialize` on
//! anything at all -- a verified identity must never be reachable from a wire DTO, and
//! `#[derive(TS)]` on a struct containing one fails to compile for want of a `TS` impl.
//!
//! # Naming rule for anything added here
//!
//! A type name must answer WHO may do WHAT to WHOM. If two authorities differ in any of
//! those three, they are two types with two names -- never one type with a boolean or a
//! string discriminant. Short nouns (`Ban`, `Role`, `Admin`, `Permission`, `Owner`,
//! `Moderator`, `Signer`, `Scope`) are banned here: each of them already means between two
//! and twenty-four unrelated things elsewhere in this workspace, and that smearing is the
//! defect this crate exists to make unrepresentable. Verbosity is cheap. Wrap the line.
//!
//! # The chokepoint
//!
//! [`VerifiedWalletAddress::from_verified_signed_fetch`]
//! is the only function in the workspace that produces a verified human identity. See its
//! documentation for why it cannot be bypassed and for what it does *not* prove.
//!
//! # What this crate does not cover
//!
//! Seven verifiers in the workspace mint an identity without going through
//! `catalyrst_crypto::signed_fetch`. They still mint bare `String`s and none of them is
//! migrated. They are enumerated -- with their weaknesses stated as facts, not comments --
//! by [`NonSharedAuthVerifier`], which mints nothing.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::wildcard_enum_match_arm)]

mod claimed;
mod operator_configured_allowlist;
mod platform_service_identity;
mod principal;
mod refusal;
mod verified_wallet_address;
mod verifier_registry;

pub use claimed::{
    ClaimedCommunityRoleNameNobodyHasVerified, ClaimedWalletAddressNobodyHasVerified,
    UnverifiedAdminDisplayName, UnverifiedOperatorDisplayName,
};
pub use operator_configured_allowlist::ConfiguredWalletAllowlist;
pub use platform_service_identity::{
    establish_platform_service_identity_by_comparing_presented_shared_secret,
    AuthenticatedPlatformServiceIdentity,
};
pub use principal::AuthenticatedPrincipal;
pub use refusal::AuthorityNotEstablished;
pub use verified_wallet_address::VerifiedWalletAddress;
pub use verifier_registry::NonSharedAuthVerifier;
