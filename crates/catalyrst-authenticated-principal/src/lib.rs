//! The shared vocabulary for **who** a request is from, and **by what mechanism** that
//! was established.
//!
//! One *principal* vocabulary, shared workspace-wide: human wallet, platform service, peer
//! catalyst server, external system. Until now each mechanism answered that with a bare
//! `String` or an `Ok(())`.
//!
//! It is **not** a permission model. There is no `Authorized<Scope, Capability>`, no
//! capability registry and no table of tenancy scopes; `docs/authz-confusion-defense.md`
//! rejects those and this crate does not reintroduce them. Every "may WHO do WHAT to WHOM"
//! type stays crate-local to the domain that owns the question, with its own long explicit
//! name. This crate shares only the *principal* and the *shape of a refusal*.
//!
//! It performs no I/O. It has no `sqlx`, no `ts-rs`, no `utoipa`, and no `Serialize` on
//! anything at all -- a verified identity must never be reachable from a wire DTO, and
//! `#[derive(TS)]` on a struct containing one fails to compile for want of a `TS` impl.
//!
//! Naming rule for anything added here: a type name must answer WHO may do WHAT to WHOM, and
//! two authorities differing in any of the three are two types with two names -- never one
//! type with a boolean or a string discriminant. Short nouns (`Ban`, `Role`, `Admin`,
//! `Permission`, `Owner`, `Moderator`, `Signer`, `Scope`) are banned here: each already means
//! between two and twenty-four unrelated things elsewhere in this workspace, and that smearing
//! is the defect this crate exists to make unrepresentable. Verbosity is cheap. Wrap the line.
//!
//! [`VerifiedWalletAddress::from_verified_signed_fetch`] is the only function in the workspace
//! that produces a verified human identity; see its documentation for why it cannot be
//! bypassed and for what it does *not* prove. Seven other verifiers mint an identity without
//! going through `catalyrst_crypto::signed_fetch`, still as bare `String`s and none migrated;
//! they are enumerated, with their weaknesses stated as facts rather than comments, by
//! [`NonSharedAuthVerifier`], which mints nothing.

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
