//! The standing one wallet holds in one community, and the single parse of a stored
//! role string in this crate.
//!
//! Before this module there were five separate answers to "what is this wallet's role in
//! this community", each with its own vocabulary:
//!
//! | where | vocabulary | on a DB error |
//! |---|---|---|
//! | `rest::fed::authority::load_role` | `owner`/`mod`/`moderator`/`member`/`banned`/`none`/`""` | propagates |
//! | `rest::handlers::client::mod::load_role_uuid` | the same set | **swallowed**, degraded to "not a member" |
//! | `rest::handlers::roles::has_moderation_permission` | `owner`/`moderator`/`mod`/**`admin`** | n/a (took a `String`) |
//! | `rest::handlers::writes::requests::role_has_invite_users` | `owner`/`moderator`/`mod` | propagates |
//! | `rest::ports::communities::member_role` | none -- returned the raw `String` | propagates |
//!
//! All five now parse here, once.

#![deny(clippy::wildcard_enum_match_arm)]

use catalyrst_authenticated_principal::AuthorityNotEstablished;
use sqlx::PgPool;
use uuid::Uuid;

use crate::rest::handlers::permissions::{has_permission, Permission};

/// The standing one wallet holds in one community.
///
/// # What a value of this type proves
///
/// That a row (or the documented absence of a row) in one named table said this. It is a
/// *fact read from a store*, not a decision: see
/// [`super::ban_authority::CommunityBanAuthority`] for
/// what a decision looks like.
///
/// # What it does NOT prove
///
/// - Not that the wallet was authenticated. Pair it with a
///   [`catalyrst_authenticated_principal::VerifiedWalletAddress`].
/// - Not that the *other* table agrees. The client and federation paths read two
///   different tables; see [`CommunityMembershipTierSourceTable`].
///
/// # `Ord` is load-bearing
///
/// The variants are declared least-to-most authority, and `actual < minimum` is the whole
/// of the old `require_min_role`. Reordering them changes authorization.
///
/// This replaces `rest::fed::authority::Role`, variant for variant and ordering for
/// ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommunityMembershipTier {
    /// Explicitly banned. Lowest, so `tier_is_at_least` refuses it for every minimum.
    BannedFromThisCommunity,
    /// No membership row, or a stored value this crate does not recognise.
    NotAMemberOfThisCommunity,
    /// A member with no moderation capability.
    OrdinaryMemberOfThisCommunity,
    /// A moderator of this one community. **Not** a global platform moderator
    /// (`catalyrst-comms`' `authorize_moderator`), and **not** a scene administrator.
    ModeratorOfThisCommunity,
    /// The owner of this one community. **Not** a world-name owner, a LAND owner, or a
    /// collection owner.
    OwnerOfThisCommunity,
}

impl CommunityMembershipTier {
    /// The only parse of a role string *read out of a table* in this crate.
    ///
    /// Total: an unrecognised value is [`Self::NotAMemberOfThisCommunity`], which holds
    /// no capability. Behaviour-identical to the old
    /// `Role::parse(raw).unwrap_or(Role::None)`, including the rejection of the removed
    /// `"admin"` tier that `rest::fed::authority`'s `admin_tier_is_removed` test pins.
    pub fn parse_role_text_as_stored_in_a_table(raw: &str) -> Self {
        Self::parse_role_text_supplied_in_a_request(raw).unwrap_or(Self::NotAMemberOfThisCommunity)
    }

    /// The parse for a role string a *caller supplied*, where an unrecognised value must
    /// be rejected rather than silently demoted.
    ///
    /// Byte-identical to the old `Role::parse`. `"admin"` is `None` here: the tier was
    /// removed, and migration `0006_role_check_reconcile.sql` dropped it from the
    /// `community_role_current` / `community_role_log` CHECK constraints.
    pub fn parse_role_text_supplied_in_a_request(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(Self::OwnerOfThisCommunity),
            "mod" | "moderator" => Some(Self::ModeratorOfThisCommunity),
            "member" => Some(Self::OrdinaryMemberOfThisCommunity),
            "banned" => Some(Self::BannedFromThisCommunity),
            "none" | "" => Some(Self::NotAMemberOfThisCommunity),
            _ => None,
        }
    }

    /// The canonical stored spelling, as written to `community_role_current` and
    /// `community_role_log`. Byte-identical to the old `Role::as_str`.
    ///
    /// Note this is **not** the spelling the client-side `community_members` table uses
    /// for a moderator: that one is `"moderator"`, written by
    /// `rest::handlers::client::mod::stored_role`. Two tables, two spellings; naming them
    /// is [`CommunityMembershipTierSourceTable`]'s job.
    pub fn as_canonical_stored_role_text(self) -> &'static str {
        match self {
            Self::OwnerOfThisCommunity => "owner",
            Self::ModeratorOfThisCommunity => "mod",
            Self::OrdinaryMemberOfThisCommunity => "member",
            Self::BannedFromThisCommunity => "banned",
            Self::NotAMemberOfThisCommunity => "none",
        }
    }

    /// True when this tier counts as a member at all. Byte-identical to the old
    /// `permissions::is_member`.
    pub fn counts_as_a_member_of_this_community(self) -> bool {
        !matches!(
            self,
            Self::NotAMemberOfThisCommunity | Self::BannedFromThisCommunity
        )
    }

    /// Whether this tier holds one capability within its own community. Delegates to the
    /// untouched 15/11 matrix in [`crate::rest::handlers::permissions`].
    pub fn holds_capability_within_this_community(self, capability: Permission) -> bool {
        has_permission(self, capability)
    }
}

/// Which table a tier was read from.
///
/// The client path and the federation path read two **different** tables with two
/// different spellings for the same tier, and until now nothing recorded which one had
/// answered. This module does not unify them -- that is a data migration, not a typing
/// change -- it names them so a reviewer reading a predicate can see which table decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityMembershipTierSourceTable {
    /// `community_role_current`, keyed by the federation's hex community id and `member`.
    /// Spells a moderator `"mod"`. CHECK-constrained to
    /// `owner|mod|member|banned|none` since migration `0006`.
    FederatedCommunityRole,
    /// `community_members`, keyed by the community UUID and `member_address`. Spells a
    /// moderator `"moderator"` when written by the client path and `"mod"` when written
    /// by `rest::fed::apply`'s role projection. **No CHECK constraint** -- the column is a
    /// bare `VARCHAR` (`migrations/0001_initial.sql`).
    CommunityMembersTableReadByTheClientPath,
}

impl CommunityMembershipTierSourceTable {
    /// The table name, for the operator log and for
    /// [`AuthorityNotEstablished::UndeterminedStoreUnavailable`].
    pub fn table_name(self) -> &'static str {
        match self {
            Self::FederatedCommunityRole => "community_role_current",
            Self::CommunityMembersTableReadByTheClientPath => "community_members",
        }
    }
}

/// A tier, the wallet it belongs to, the community it was read for, the table it came
/// from, and the raw text as stored. All five travel together so a reviewer reading a
/// predicate can see which table answered and what it actually held.
#[derive(Debug, Clone)]
pub struct CommunityMembershipStanding {
    community_identifier_text: String,
    wallet_address_lowercase: String,
    tier: CommunityMembershipTier,
    source_table: CommunityMembershipTierSourceTable,
    /// `None` when no row existed. Distinguishing "no row" from "a row holding something
    /// unrecognised" is required to reproduce
    /// `rest::ports::communities::member_role`'s `Option<String>` exactly.
    stored_role_text_if_a_row_exists: Option<String>,
}

impl CommunityMembershipStanding {
    fn from_row(
        community_identifier_text: String,
        wallet_address_lowercase: String,
        source_table: CommunityMembershipTierSourceTable,
        stored_role_text_if_a_row_exists: Option<String>,
    ) -> Self {
        let tier = match stored_role_text_if_a_row_exists.as_deref() {
            Some(raw) => CommunityMembershipTier::parse_role_text_as_stored_in_a_table(raw),
            None => CommunityMembershipTier::NotAMemberOfThisCommunity,
        };
        let standing = Self {
            community_identifier_text,
            wallet_address_lowercase,
            tier,
            source_table,
            stored_role_text_if_a_row_exists,
        };
        standing.warn_if_a_removed_or_unrecognised_tier_was_read();
        standing
    }

    /// Instrumentation for the one behaviour change this module makes to a stored value.
    ///
    /// `rest::handlers::roles::has_moderation_permission` used to accept the literal
    /// `"admin"` as a moderator, on the `community_members` table only. Nothing in this
    /// workspace writes that value -- the two federation tables have CHECK-constrained it
    /// away since migration `0006_role_check_reconcile.sql`, whose own comment calls it
    /// "the never-used legacy 'admin' value", and every `INSERT`/`UPDATE` of
    /// `community_members.role` in this crate binds `owner`, `member`, `mod`, or
    /// `moderator`. It is nonetheless a bare `VARCHAR` with no CHECK, so an imported row
    /// could in principle hold it.
    ///
    /// Rather than keep accepting it, the acceptance is removed and a live occurrence is
    /// made loud: this warns once per read, with the community and the wallet, so the
    /// claim "nothing stores this" is falsifiable in production rather than asserted.
    fn warn_if_a_removed_or_unrecognised_tier_was_read(&self) {
        let Some(raw) = self.stored_role_text_if_a_row_exists.as_deref() else {
            return;
        };
        if CommunityMembershipTier::parse_role_text_supplied_in_a_request(raw).is_some() {
            return;
        }
        tracing::warn!(
            table = self.source_table.table_name(),
            community = %self.community_identifier_text,
            wallet = %self.wallet_address_lowercase,
            stored_role_text = %raw,
            "community membership row holds a role this service does not recognise; \
             treating it as not a member. If this is 'admin', it is the removed legacy \
             tier that has_moderation_permission used to accept."
        );
    }

    /// The tier itself.
    pub fn tier(&self) -> CommunityMembershipTier {
        self.tier
    }

    /// Which table answered.
    pub fn source_table(&self) -> CommunityMembershipTierSourceTable {
        self.source_table
    }

    /// The community this standing was read for: the federation hex id or the UUID text,
    /// depending on [`Self::source_table`].
    pub fn community_identifier_text(&self) -> &str {
        &self.community_identifier_text
    }

    /// The wallet, lowercased exactly as it was bound into the query.
    pub fn wallet_address_lowercase(&self) -> &str {
        &self.wallet_address_lowercase
    }

    /// Whether a membership row existed at all, irrespective of what it held.
    ///
    /// This is the exact replacement for the old
    /// `state.communities.member_role(..).is_none()` presence check in
    /// `rest::handlers::members`. It deliberately does **not** consult the tier: a row
    /// holding an unrecognised string used to pass that check, and still does.
    pub fn a_membership_row_exists_for_this_wallet(&self) -> bool {
        self.stored_role_text_if_a_row_exists.is_some()
    }

    /// The raw stored text, or `"none"` when no row existed.
    ///
    /// Reproduces `rest::handlers::writes::requests::member_role_str` exactly, including
    /// its `"none"` default, so the "already a member" check keeps its meaning.
    pub fn stored_role_text_defaulting_to_none_when_no_row_exists(&self) -> &str {
        self.stored_role_text_if_a_row_exists
            .as_deref()
            .unwrap_or("none")
    }

    /// `actual >= minimum`. This is the whole of the old `require_min_role`'s tier test.
    pub fn tier_is_at_least(&self, minimum: CommunityMembershipTier) -> bool {
        self.tier >= minimum
    }

    /// Whether this wallet holds one capability in this community.
    pub fn holds_capability_within_this_community(&self, capability: Permission) -> bool {
        self.tier.holds_capability_within_this_community(capability)
    }

    /// Whether this wallet counts as a member at all.
    pub fn counts_as_a_member_of_this_community(&self) -> bool {
        self.tier.counts_as_a_member_of_this_community()
    }
}

/// Turn a query failure into "we could not tell", never into "no".
///
/// `AuthorityNotEstablished` lives in `catalyrst-authenticated-principal`, which has no
/// `sqlx`, so the conversion is written here -- the orphan rule would block a `From` impl
/// anyway, and a free function keeps the store name at the call site.
pub(crate) fn undetermined_because_the_backing_store_was_unavailable(
    source_table: CommunityMembershipTierSourceTable,
    error: sqlx::Error,
) -> AuthorityNotEstablished {
    AuthorityNotEstablished::UndeterminedStoreUnavailable {
        store: source_table.table_name(),
        reason_for_operators: error.to_string(),
    }
}

/// Read a standing from `community_role_current` -- the **federation** path's table.
///
/// Replaces `rest::fed::authority::load_role`: same `to_ascii_lowercase` on the member, same
/// "missing row means not a member", and the same propagation of a query failure. The one added
/// behaviour is the soft-delete guard documented on the query below (upstream #460, extended to the
/// federation table).
pub async fn load_standing_from_community_role_current(
    pool: &PgPool,
    community_id_hex_text: &str,
    wallet_address: &str,
) -> Result<CommunityMembershipStanding, AuthorityNotEstablished> {
    const SOURCE: CommunityMembershipTierSourceTable =
        CommunityMembershipTierSourceTable::FederatedCommunityRole;
    let wallet_address_lowercase = wallet_address.to_ascii_lowercase();
    // The federation delete (`rest::fed::apply::apply_delete`) flips the projected
    // `communities.active` to false while leaving `community_role_current` in place, so this loader
    // has the same privilege-retention gap #460 fixed on the client path: an ex-owner of a deleted
    // federated community keeps their role and it still backs ban_authority / post / kick-ban-role
    // decisions. Unlike `community_members`, this table is keyed by the hex id with no FK to
    // `communities`, so a role row can legitimately exist before this node has materialised the
    // community. The guard therefore excludes only an *explicitly* soft-deleted community -- a
    // present `communities` row with active = false, which is exactly what apply_delete produces --
    // and stays fail-open when no row exists yet, rather than the client path's require-active form.
    let community_uuid = crate::rest::fed::ids::community_uuid_from_hex(community_id_hex_text);
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM community_role_current crc \
         WHERE crc.community_id = $1 AND crc.member = $2 \
           AND NOT EXISTS (SELECT 1 FROM communities c WHERE c.id = $3 AND c.active = false)",
    )
    .bind(community_id_hex_text)
    .bind(&wallet_address_lowercase)
    .bind(community_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|e| undetermined_because_the_backing_store_was_unavailable(SOURCE, e))?;
    Ok(CommunityMembershipStanding::from_row(
        community_id_hex_text.to_string(),
        wallet_address_lowercase,
        SOURCE,
        row.map(|(r,)| r),
    ))
}

/// Read a standing from `community_members` -- the **client** path's table.
///
/// Replaces `rest::handlers::client::mod::load_role_uuid`,
/// `rest::ports::communities::member_role` and
/// `rest::handlers::writes::requests::member_role_str`, which ran the same query three
/// times with three different error policies.
///
/// **Deliberate behaviour change (BC-1).** `load_role_uuid` ended its query with
/// `.ok().flatten()`, so a SQL fault read as "not a member". That is fail-open at the
/// *target* of a moderation action: with the actor's own lookup succeeding, a failed
/// target lookup demoted the target to "not a member", and `can_act_on_member`'s
/// `!is_member(target)` escape then permitted the action against a community owner. This
/// loader has no `Option` and no `bool` in its error position, so there is nothing for
/// `.unwrap_or(false)` to apply to. It is the same policy the federation path has always
/// had, and the same shape as the `is_banned_uuid` fix already in the tree
/// (`security/fail-open-authz-fixes`, `ac45717d6`).
pub async fn load_standing_from_community_members(
    pool: &PgPool,
    community_id: Uuid,
    wallet_address: &str,
) -> Result<CommunityMembershipStanding, AuthorityNotEstablished> {
    const SOURCE: CommunityMembershipTierSourceTable =
        CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath;
    let wallet_address_lowercase = wallet_address.to_lowercase();
    // Membership rows survive a soft delete (`communities.active = false`), so without the active
    // guard a deleted community's former staff still resolve as owners/moderators and every
    // client-path moderation decision keyed on this standing still passes (upstream #460). The FK
    // from community_members to communities makes requiring an active row exact.
    let row: Option<String> = sqlx::query_scalar(
        "SELECT role FROM community_members \
         WHERE community_id = $1 AND member_address = $2 \
           AND EXISTS (SELECT 1 FROM communities c WHERE c.id = $1 AND c.active = true)",
    )
    .bind(community_id)
    .bind(&wallet_address_lowercase)
    .fetch_optional(pool)
    .await
    .map_err(|e| undetermined_because_the_backing_store_was_unavailable(SOURCE, e))?;
    Ok(CommunityMembershipStanding::from_row(
        community_id.to_string(),
        wallet_address_lowercase,
        SOURCE,
        row,
    ))
}

#[cfg(test)]
pub(crate) fn standing_for_tests(
    tier_text: &str,
    a_row_exists: bool,
    source_table: CommunityMembershipTierSourceTable,
) -> CommunityMembershipStanding {
    CommunityMembershipStanding::from_row(
        "0xcommunity".to_string(),
        "0xwallet".to_string(),
        source_table,
        a_row_exists.then(|| tier_text.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::CommunityMembershipTier as Tier;
    use super::*;

    /// Every role string either table can hold, plus the removed tier and the shapes a
    /// bare `VARCHAR` column admits.
    const EVERY_ROLE_TEXT_A_TABLE_COULD_HOLD: &[&str] = &[
        "owner",
        "moderator",
        "mod",
        "admin",
        "member",
        "banned",
        "none",
        "",
        "ADMIN",
        "Owner",
        "whatever",
    ];

    #[test]
    fn the_removed_admin_tier_does_not_parse() {
        assert_eq!(Tier::parse_role_text_supplied_in_a_request("admin"), None);
        assert_eq!(
            Tier::parse_role_text_as_stored_in_a_table("admin"),
            Tier::NotAMemberOfThisCommunity
        );
        for tier in [
            Tier::OwnerOfThisCommunity,
            Tier::ModeratorOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::BannedFromThisCommunity,
            Tier::NotAMemberOfThisCommunity,
        ] {
            assert_ne!(tier.as_canonical_stored_role_text(), "admin");
        }
    }

    #[test]
    fn role_text_round_trips_exactly_as_the_old_role_enum_did() {
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("owner"),
            Some(Tier::OwnerOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("moderator"),
            Some(Tier::ModeratorOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("mod"),
            Some(Tier::ModeratorOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("member"),
            Some(Tier::OrdinaryMemberOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("banned"),
            Some(Tier::BannedFromThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("none"),
            Some(Tier::NotAMemberOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request(""),
            Some(Tier::NotAMemberOfThisCommunity)
        );
        assert_eq!(
            Tier::OwnerOfThisCommunity.as_canonical_stored_role_text(),
            "owner"
        );
        assert_eq!(
            Tier::OrdinaryMemberOfThisCommunity.as_canonical_stored_role_text(),
            "member"
        );
    }

    #[test]
    fn tier_ordering_is_the_authorization_ordering() {
        assert!(Tier::BannedFromThisCommunity < Tier::NotAMemberOfThisCommunity);
        assert!(Tier::NotAMemberOfThisCommunity < Tier::OrdinaryMemberOfThisCommunity);
        assert!(Tier::OrdinaryMemberOfThisCommunity < Tier::ModeratorOfThisCommunity);
        assert!(Tier::ModeratorOfThisCommunity < Tier::OwnerOfThisCommunity);
    }

    /// P10 -- the four moderation predicates that used to disagree, compared over every
    /// role string a table can hold.
    ///
    /// The three legacy predicates are reproduced here verbatim, as they stood before
    /// this module existed, so that the table records the exact delta rather than
    /// asserting the new code agrees with itself.
    #[test]
    fn the_four_legacy_moderation_predicates_now_agree_except_on_the_removed_admin_tier() {
        /// `rest::handlers::roles::has_moderation_permission`, verbatim.
        fn legacy_has_moderation_permission(role: Option<&str>) -> bool {
            matches!(
                role,
                Some("owner") | Some("moderator") | Some("mod") | Some("admin")
            )
        }
        /// `rest::handlers::writes::requests::role_has_invite_users`, verbatim.
        fn legacy_role_has_invite_users(role: &str) -> bool {
            matches!(role, "owner" | "moderator" | "mod")
        }
        /// `rpc::service::domain::require_moderator`'s inner test, verbatim.
        fn legacy_rpc_require_moderator(role: &str) -> bool {
            role == "owner" || role == "moderator"
        }

        let mut disagreements = Vec::new();
        for raw in EVERY_ROLE_TEXT_A_TABLE_COULD_HOLD {
            let tier = Tier::parse_role_text_as_stored_in_a_table(raw);
            let tiered_moderation =
                tier.holds_capability_within_this_community(Permission::BanPlayers);
            let tiered_invite =
                tier.holds_capability_within_this_community(Permission::InviteUsers);

            if tiered_moderation != legacy_has_moderation_permission(Some(raw)) {
                disagreements.push(format!("has_moderation_permission({raw:?})"));
            }
            if tiered_invite != legacy_role_has_invite_users(raw) {
                disagreements.push(format!("role_has_invite_users({raw:?})"));
            }
            if tiered_moderation != legacy_rpc_require_moderator(raw) {
                disagreements.push(format!("rpc require_moderator({raw:?})"));
            }
        }

        assert_eq!(
            disagreements,
            vec![
                // The RPC predicate never accepted `"mod"`, which `rest::fed::apply`
                // projects into `community_members` for a federated moderator. Migrating
                // it onto the tier fixes that; before the migration this row was the
                // evidence. (BC-4)
                "rpc require_moderator(\"mod\")".to_string(),
                // The removed tier. `has_moderation_permission` accepted it; the tiered
                // predicate does not. (BC-5)
                "has_moderation_permission(\"admin\")".to_string(),
            ],
            "the moderation predicates disagree somewhere other than the two known, \
             deliberate deltas"
        );
    }

    /// P12 -- adding a capability to `MODERATOR_PERMISSIONS` without adding it to
    /// `OWNER_PERMISSIONS` would make the matrix non-monotone in tier. The existing 15/11
    /// length assertions do not catch that.
    #[test]
    fn the_capability_matrix_is_monotone_in_tier() {
        const EVERY_TIER: &[Tier] = &[
            Tier::BannedFromThisCommunity,
            Tier::NotAMemberOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::ModeratorOfThisCommunity,
            Tier::OwnerOfThisCommunity,
        ];
        const EVERY_CAPABILITY: &[Permission] = &[
            Permission::EditInfo,
            Permission::EditName,
            Permission::AddPlaces,
            Permission::RemovePlaces,
            Permission::AcceptRequests,
            Permission::RejectRequests,
            Permission::ViewRequests,
            Permission::BanPlayers,
            Permission::SendInvitations,
            Permission::EditSettings,
            Permission::DeleteCommunity,
            Permission::AssignRoles,
            Permission::InviteUsers,
            Permission::CreatePosts,
            Permission::DeletePosts,
        ];
        assert_eq!(
            EVERY_CAPABILITY.len(),
            15,
            "a Permission variant was added without a row here"
        );
        for higher in EVERY_TIER {
            for lower in EVERY_TIER {
                if higher < lower {
                    continue;
                }
                for capability in EVERY_CAPABILITY {
                    assert!(
                        !lower.holds_capability_within_this_community(*capability)
                            || higher.holds_capability_within_this_community(*capability),
                        "{lower:?} holds {capability:?} but the higher tier {higher:?} does not"
                    );
                }
            }
        }
    }

    #[test]
    fn a_missing_row_is_distinguishable_from_a_row_holding_an_unrecognised_value() {
        let no_row = standing_for_tests(
            "",
            false,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );
        let junk_row = standing_for_tests(
            "wat",
            true,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );

        assert!(!no_row.a_membership_row_exists_for_this_wallet());
        assert!(junk_row.a_membership_row_exists_for_this_wallet());
        assert_eq!(no_row.tier(), junk_row.tier());
        assert_eq!(
            no_row.stored_role_text_defaulting_to_none_when_no_row_exists(),
            "none"
        );
        assert_eq!(
            junk_row.stored_role_text_defaulting_to_none_when_no_row_exists(),
            "wat"
        );
    }

    #[test]
    fn the_source_table_travels_with_the_standing() {
        let federated = standing_for_tests(
            "mod",
            true,
            CommunityMembershipTierSourceTable::FederatedCommunityRole,
        );
        let client = standing_for_tests(
            "moderator",
            true,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );
        assert_eq!(federated.tier(), client.tier());
        assert_ne!(federated.source_table(), client.source_table());
        assert_eq!(
            federated.source_table().table_name(),
            "community_role_current"
        );
        assert_eq!(client.source_table().table_name(), "community_members");
    }
}
