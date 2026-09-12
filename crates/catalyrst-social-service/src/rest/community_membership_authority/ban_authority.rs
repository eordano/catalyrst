//! The authority to ban, or to lift a ban on, one wallet in **one community**.
//!
//! `ban` names nine unrelated authorities across this workspace. The nearest others are a
//! **scene** ban from one place (`catalyrst-comms`' `handlers::scene_bans`), a **global
//! platform** ban (`catalyrst-comms`' `handlers::user_bans`) and a **world** ban
//! (`catalyrst-worlds`' `ports::bans`). None is interchangeable with another.
//!
//! All three community-ban entry points -- `client::members`, `writes::members` and
//! `fed::consumer` -- reach
//! [`decide_whether_the_acting_party_outranks_the_target`], and nothing else does.

#![deny(clippy::wildcard_enum_match_arm)]

use catalyrst_authenticated_principal::{AuthorityNotEstablished, VerifiedWalletAddress};
use sqlx::PgPool;
use uuid::Uuid;

use crate::rest::handlers::permissions::{can_act_on_member, Permission};

use super::standing::{
    load_standing_from_community_members, load_standing_from_community_role_current,
    CommunityMembershipStanding, CommunityMembershipTierSourceTable,
};

/// Records provenance so the three authentications cannot smear into one another. Never
/// escapes this module.
#[derive(Debug)]
enum HowTheActingPartyWasAuthenticated {
    /// An end user's ADR-44 signed fetch, verified on this node against this request's
    /// method, path, timestamp and metadata.
    EndUserAdr44SignedFetchOnThisNode(VerifiedWalletAddress),
    /// A federation envelope whose EIP-712 signature was checked against the wallet that
    /// also completed the outer signed fetch on this node.
    EnvelopeSignerMatchesOuterSigner(VerifiedWalletAddress),
    /// The originating wallet is **recovered from the envelope's own EIP-712 signature**,
    /// not established through the shared ADR-44 chokepoint, so it arrives as text and
    /// cannot be minted into a [`VerifiedWalletAddress`].
    PeerRelayedGossipEnvelope {
        originating_wallet_address_lowercase: String,
    },
}

impl HowTheActingPartyWasAuthenticated {
    fn acting_wallet_address_lowercase(&self) -> String {
        match self {
            Self::EndUserAdr44SignedFetchOnThisNode(wallet)
            | Self::EnvelopeSignerMatchesOuterSigner(wallet) => {
                wallet.as_lowercased_hex_text().to_string()
            }
            Self::PeerRelayedGossipEnvelope {
                originating_wallet_address_lowercase,
            } => originating_wallet_address_lowercase.clone(),
        }
    }

    fn audit_actor_description(&self) -> String {
        match self {
            Self::EndUserAdr44SignedFetchOnThisNode(wallet) => format!("wallet:{wallet}"),
            Self::EnvelopeSignerMatchesOuterSigner(wallet) => {
                format!("federation-envelope-wallet:{wallet}")
            }
            Self::PeerRelayedGossipEnvelope {
                originating_wallet_address_lowercase,
            } => format!("gossip-relayed-wallet:{originating_wallet_address_lowercase}"),
        }
    }
}

/// THE predicate. Both moderation authorities in this module reach it, from all three of
/// their constructors, and nothing else in the crate does. It **tightens**
/// `fed::consumer`'s former `CommunityBan` arm, which checked the acting party's tier and
/// never looked at the target's.
fn decide_whether_the_acting_party_outranks_the_target(
    acting_party_standing: &CommunityMembershipStanding,
    target_wallet_standing: &CommunityMembershipStanding,
    what_the_acting_party_is_trying_to_do: &'static str,
) -> Result<(), AuthorityNotEstablished> {
    if !acting_party_standing.holds_capability_within_this_community(Permission::BanPlayers) {
        return Err(AuthorityNotEstablished::RefusedLacksAuthority {
            detail: format!(
                "{} holds the {} tier in community {}, which cannot {} in it",
                acting_party_standing.wallet_address_lowercase(),
                acting_party_standing.tier().as_canonical_stored_role_text(),
                acting_party_standing.community_identifier_text(),
                what_the_acting_party_is_trying_to_do,
            ),
        });
    }
    if !can_act_on_member(acting_party_standing.tier(), target_wallet_standing.tier())
        && target_wallet_standing.counts_as_a_member_of_this_community()
    {
        return Err(AuthorityNotEstablished::RefusedLacksAuthority {
            detail: format!(
                "{} may not {} {}, who holds the {} tier in community {}",
                acting_party_standing.wallet_address_lowercase(),
                what_the_acting_party_is_trying_to_do,
                target_wallet_standing.wallet_address_lowercase(),
                target_wallet_standing
                    .tier()
                    .as_canonical_stored_role_text(),
                acting_party_standing.community_identifier_text(),
            ),
        });
    }
    Ok(())
}

/// A query failure becomes [`AuthorityNotEstablished::UndeterminedStoreUnavailable`] and
/// never a refusal, so an outage cannot read as a decision in either direction.
async fn resolve_moderation_authority_over_one_member(
    pool: &PgPool,
    how_the_acting_party_was_authenticated: HowTheActingPartyWasAuthenticated,
    community_identifier: CommunityIdentifierForTheTableBeingRead<'_>,
    target_wallet_address: &str,
    what_the_acting_party_is_trying_to_do: &'static str,
) -> Result<ResolvedModerationAuthorityOverOneMember, AuthorityNotEstablished> {
    let acting_wallet_address_lowercase =
        how_the_acting_party_was_authenticated.acting_wallet_address_lowercase();

    let (acting_party_standing, target_wallet_standing) = match community_identifier {
        CommunityIdentifierForTheTableBeingRead::CommunityUuidForTheCommunityMembersTable(uuid) => {
            (
                load_standing_from_community_members(pool, uuid, &acting_wallet_address_lowercase)
                    .await?,
                load_standing_from_community_members(pool, uuid, target_wallet_address).await?,
            )
        }
        CommunityIdentifierForTheTableBeingRead::FederatedCommunityHexId(hex) => (
            load_standing_from_community_role_current(pool, hex, &acting_wallet_address_lowercase)
                .await?,
            load_standing_from_community_role_current(pool, hex, target_wallet_address).await?,
        ),
    };

    decide_whether_the_acting_party_outranks_the_target(
        &acting_party_standing,
        &target_wallet_standing,
        what_the_acting_party_is_trying_to_do,
    )?;

    Ok(ResolvedModerationAuthorityOverOneMember {
        community_identifier_text: acting_party_standing
            .community_identifier_text()
            .to_string(),
        how_the_acting_party_was_authenticated,
        acting_party_standing,
        target_wallet_address_lowercase: target_wallet_standing
            .wallet_address_lowercase()
            .to_string(),
        target_wallet_standing,
    })
}

/// Which key identifies the community, and therefore which table is read -- one type
/// rather than two parameters that could disagree.
#[derive(Debug, Clone, Copy)]
enum CommunityIdentifierForTheTableBeingRead<'a> {
    /// The client path's UUID key into `community_members`.
    CommunityUuidForTheCommunityMembersTable(Uuid),
    /// The federation path's hex id key into `community_role_current`.
    FederatedCommunityHexId(&'a str),
}

/// Not public: a caller must name which authority it holds, not this.
#[derive(Debug)]
struct ResolvedModerationAuthorityOverOneMember {
    community_identifier_text: String,
    how_the_acting_party_was_authenticated: HowTheActingPartyWasAuthenticated,
    acting_party_standing: CommunityMembershipStanding,
    target_wallet_address_lowercase: String,
    target_wallet_standing: CommunityMembershipStanding,
}

macro_rules! shared_accessors {
    () => {
        /// **One** community; the type carries no power over any other.
        pub fn community_identifier_text(&self) -> &str {
            &self.0.community_identifier_text
        }

        pub fn target_wallet_address_lowercase(&self) -> &str {
            &self.0.target_wallet_address_lowercase
        }

        /// What the write should record as `banned_by` / `unbanned_by`.
        pub fn acting_wallet_address_lowercase(&self) -> String {
            self.0
                .how_the_acting_party_was_authenticated
                .acting_wallet_address_lowercase()
        }

        /// Names both the wallet and **how it was established**, so an audit row cannot
        /// claim more than the server proved.
        pub fn audit_actor_description(&self) -> String {
            self.0
                .how_the_acting_party_was_authenticated
                .audit_actor_description()
        }

        pub fn acting_party_standing(&self) -> &CommunityMembershipStanding {
            &self.0.acting_party_standing
        }

        pub fn target_wallet_standing(&self) -> &CommunityMembershipStanding {
            &self.0.target_wallet_standing
        }

        pub fn source_table(&self) -> CommunityMembershipTierSourceTable {
            self.0.acting_party_standing.source_table()
        }
    };
}

/// A value exists only downstream of
/// [`decide_whether_the_acting_party_outranks_the_target`]. Holding one is the proof; the
/// write that follows does not re-derive anything.
#[derive(Debug)]
pub struct CommunityBanAuthority(ResolvedModerationAuthorityOverOneMember);

impl CommunityBanAuthority {
    const WHAT: &'static str = "ban";

    /// The client write path: keyed by UUID, so `community_members` answers.
    pub async fn resolve_from_end_user_signed_fetch_on_this_node(
        pool: &PgPool,
        banning_wallet: &VerifiedWalletAddress,
        community_id: Uuid,
        target_wallet_address: &str,
    ) -> Result<Self, AuthorityNotEstablished> {
        resolve_moderation_authority_over_one_member(
            pool,
            HowTheActingPartyWasAuthenticated::EndUserAdr44SignedFetchOnThisNode(
                banning_wallet.clone(),
            ),
            CommunityIdentifierForTheTableBeingRead::CommunityUuidForTheCommunityMembersTable(
                community_id,
            ),
            target_wallet_address,
            Self::WHAT,
        )
        .await
        .map(Self)
    }

    /// An EIP-712 envelope delivered over a signed fetch by the same wallet, keyed by the
    /// federation's hex community id, so `community_role_current` answers.
    pub async fn resolve_from_federation_envelope_signed_by_the_originating_wallet(
        pool: &PgPool,
        originating_wallet: &VerifiedWalletAddress,
        community_id_hex_text: &str,
        target_wallet_address: &str,
    ) -> Result<Self, AuthorityNotEstablished> {
        resolve_moderation_authority_over_one_member(
            pool,
            HowTheActingPartyWasAuthenticated::EnvelopeSignerMatchesOuterSigner(
                originating_wallet.clone(),
            ),
            CommunityIdentifierForTheTableBeingRead::FederatedCommunityHexId(community_id_hex_text),
            target_wallet_address,
            Self::WHAT,
        )
        .await
        .map(Self)
    }

    /// **Deliberate behaviour change (BC-2).** `fed::consumer`'s former `CommunityBan`
    /// arm only required the acting party to hold at least the moderator tier and never
    /// consulted the target's, so a relayed envelope could ban the community owner while
    /// both other entry points refused. It now reaches the same predicate as they do.
    pub async fn resolve_from_gossip_envelope_relayed_by_a_peer_catalyst_server(
        pool: &PgPool,
        originating_wallet_address_recovered_from_the_envelope_signature: &str,
        community_id_hex_text: &str,
        target_wallet_address: &str,
    ) -> Result<Self, AuthorityNotEstablished> {
        resolve_moderation_authority_over_one_member(
            pool,
            HowTheActingPartyWasAuthenticated::PeerRelayedGossipEnvelope {
                originating_wallet_address_lowercase:
                    originating_wallet_address_recovered_from_the_envelope_signature
                        .to_ascii_lowercase(),
            },
            CommunityIdentifierForTheTableBeingRead::FederatedCommunityHexId(community_id_hex_text),
            target_wallet_address,
            Self::WHAT,
        )
        .await
        .map(Self)
    }

    shared_accessors!();
}

/// A separate type from [`CommunityBanAuthority`] so a witness of one cannot be used as a
/// witness of the other. The predicate behind them is shared on purpose; the *names* are
/// not.
#[derive(Debug)]
pub struct CommunityUnbanAuthority(ResolvedModerationAuthorityOverOneMember);

impl CommunityUnbanAuthority {
    const WHAT: &'static str = "unban";

    /// The client write path: keyed by UUID, so `community_members` answers.
    pub async fn resolve_from_end_user_signed_fetch_on_this_node(
        pool: &PgPool,
        unbanning_wallet: &VerifiedWalletAddress,
        community_id: Uuid,
        target_wallet_address: &str,
    ) -> Result<Self, AuthorityNotEstablished> {
        resolve_moderation_authority_over_one_member(
            pool,
            HowTheActingPartyWasAuthenticated::EndUserAdr44SignedFetchOnThisNode(
                unbanning_wallet.clone(),
            ),
            CommunityIdentifierForTheTableBeingRead::CommunityUuidForTheCommunityMembersTable(
                community_id,
            ),
            target_wallet_address,
            Self::WHAT,
        )
        .await
        .map(Self)
    }

    /// Keyed by the federation's hex community id, so `community_role_current` answers.
    pub async fn resolve_from_federation_envelope_signed_by_the_originating_wallet(
        pool: &PgPool,
        originating_wallet: &VerifiedWalletAddress,
        community_id_hex_text: &str,
        target_wallet_address: &str,
    ) -> Result<Self, AuthorityNotEstablished> {
        resolve_moderation_authority_over_one_member(
            pool,
            HowTheActingPartyWasAuthenticated::EnvelopeSignerMatchesOuterSigner(
                originating_wallet.clone(),
            ),
            CommunityIdentifierForTheTableBeingRead::FederatedCommunityHexId(community_id_hex_text),
            target_wallet_address,
            Self::WHAT,
        )
        .await
        .map(Self)
    }

    /// Behaviour-preserving, unlike [`CommunityBanAuthority`]'s gossip arm (BC-2): the
    /// former `CommunityUnban` arm already spelled the shared predicate.
    pub async fn resolve_from_gossip_envelope_relayed_by_a_peer_catalyst_server(
        pool: &PgPool,
        originating_wallet_address_recovered_from_the_envelope_signature: &str,
        community_id_hex_text: &str,
        target_wallet_address: &str,
    ) -> Result<Self, AuthorityNotEstablished> {
        resolve_moderation_authority_over_one_member(
            pool,
            HowTheActingPartyWasAuthenticated::PeerRelayedGossipEnvelope {
                originating_wallet_address_lowercase:
                    originating_wallet_address_recovered_from_the_envelope_signature
                        .to_ascii_lowercase(),
            },
            CommunityIdentifierForTheTableBeingRead::FederatedCommunityHexId(community_id_hex_text),
            target_wallet_address,
            Self::WHAT,
        )
        .await
        .map(Self)
    }

    shared_accessors!();
}

#[cfg(test)]
mod tests {
    use super::super::standing::{standing_for_tests, CommunityMembershipTierSourceTable};
    use super::*;
    use crate::rest::community_membership_authority::CommunityMembershipTier as Tier;
    use crate::rest::handlers::permissions::{has_permission, is_member};

    const EVERY_TIER_TEXT: &[&str] = &["banned", "none", "member", "mod", "owner"];

    fn decide(actor_text: &str, target_text: &str) -> bool {
        let actor = standing_for_tests(
            actor_text,
            true,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );
        let target = standing_for_tests(
            target_text,
            true,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );
        decide_whether_the_acting_party_outranks_the_target(&actor, &target, "ban").is_ok()
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn the_shared_predicate_matches_the_client_paths_former_inline_predicate() {
        for actor_text in EVERY_TIER_TEXT {
            for target_text in EVERY_TIER_TEXT {
                let actor = Tier::parse_role_text_as_stored_in_a_table(actor_text);
                let target = Tier::parse_role_text_as_stored_in_a_table(target_text);
                let former_client_predicate = has_permission(actor, Permission::BanPlayers)
                    && !(!can_act_on_member(actor, target) && is_member(target));
                assert_eq!(
                    decide(actor_text, target_text),
                    former_client_predicate,
                    "actor {actor_text} target {target_text}"
                );
            }
        }
    }

    #[test]
    fn the_shared_predicate_matches_the_federation_paths_former_inline_predicate() {
        for actor_text in EVERY_TIER_TEXT {
            for target_text in EVERY_TIER_TEXT {
                let actor = Tier::parse_role_text_as_stored_in_a_table(actor_text);
                let target = Tier::parse_role_text_as_stored_in_a_table(target_text);
                let former_fed_predicate = actor != Tier::BannedFromThisCommunity
                    && actor >= Tier::ModeratorOfThisCommunity
                    && target < actor;
                assert_eq!(
                    decide(actor_text, target_text),
                    former_fed_predicate,
                    "actor {actor_text} target {target_text}"
                );
            }
        }
    }

    #[test]
    fn the_gossip_arms_former_predicate_was_more_permissive_in_exactly_these_cases() {
        let mut tightened = Vec::new();
        for actor_text in EVERY_TIER_TEXT {
            for target_text in EVERY_TIER_TEXT {
                let actor = Tier::parse_role_text_as_stored_in_a_table(actor_text);
                let former_gossip_predicate = actor != Tier::BannedFromThisCommunity
                    && actor >= Tier::ModeratorOfThisCommunity;
                if former_gossip_predicate && !decide(actor_text, target_text) {
                    tightened.push(format!("{actor_text} banning {target_text}"));
                }
            }
        }
        assert_eq!(
            tightened,
            vec![
                "mod banning mod".to_string(),
                "mod banning owner".to_string(),
                "owner banning owner".to_string(),
            ],
            "BC-2 tightened a different set of cases than documented"
        );
    }

    #[test]
    fn a_backing_store_failure_is_never_a_refusal() {
        let actor = standing_for_tests(
            "owner",
            true,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );
        let target = standing_for_tests(
            "owner",
            true,
            CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
        );
        let refusal = decide_whether_the_acting_party_outranks_the_target(&actor, &target, "ban")
            .unwrap_err();
        assert_eq!(refusal.http_status(), 403);
        assert!(matches!(
            refusal,
            AuthorityNotEstablished::RefusedLacksAuthority { .. }
        ));

        let undetermined =
            super::super::standing::undetermined_because_the_backing_store_was_unavailable(
                CommunityMembershipTierSourceTable::CommunityMembersTableReadByTheClientPath,
                sqlx::Error::PoolTimedOut,
            );
        assert_eq!(undetermined.http_status(), 503);
        assert!(matches!(
            undetermined,
            AuthorityNotEstablished::UndeterminedStoreUnavailable {
                store: "community_members",
                ..
            }
        ));
    }
}
