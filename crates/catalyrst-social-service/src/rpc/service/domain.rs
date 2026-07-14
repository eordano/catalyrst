use super::helpers::{page_friendship_requests, page_of, SocialError};
use super::server::SocialServiceImpl;
use crate::rpc::context::Context;
use crate::rpc::db::Db;
use crate::rpc::proto::errors::*;
use crate::rpc::proto::v2::*;
use crate::rpc::pubsub::SocialEvent;
use catalyrst_drpc::service_module_definition::ProcedureContext;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Action {
    Request,
    Accept,
    Cancel,
    Reject,
    Delete,
    Block,
}

impl Action {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Action::Request => "request",
            Action::Accept => "accept",
            Action::Cancel => "cancel",
            Action::Reject => "reject",
            Action::Delete => "delete",
            Action::Block => "block",
        }
    }
    fn from_str(s: &str) -> Option<Action> {
        Some(match s {
            "request" => Action::Request,
            "accept" => Action::Accept,
            "cancel" => Action::Cancel,
            "reject" => Action::Reject,
            "delete" => Action::Delete,
            "block" => Action::Block,
            _ => return None,
        })
    }

    pub(super) fn implies_active(self) -> bool {
        matches!(self, Action::Accept)
    }
}

fn transition_valid(from: Option<Action>, to: Action) -> bool {
    let allowed: &[Option<Action>] = match to {
        Action::Request => &[
            Some(Action::Cancel),
            Some(Action::Reject),
            Some(Action::Delete),
            None,
        ],
        Action::Accept => &[Some(Action::Request)],
        Action::Cancel => &[Some(Action::Request)],
        Action::Reject => &[Some(Action::Request)],
        Action::Delete => &[Some(Action::Accept), Some(Action::Block)],
        Action::Block => &[
            Some(Action::Request),
            Some(Action::Cancel),
            Some(Action::Reject),
            Some(Action::Delete),
            Some(Action::Accept),
            None,
        ],
    };
    allowed.contains(&from)
}

pub(super) fn user_action_valid(
    acting_user: &str,
    new_action: Action,
    new_user: &str,
    last: Option<&crate::rpc::db::LastAction>,
) -> bool {
    let last_act = last.and_then(|l| Action::from_str(&l.action));
    if !transition_valid(last_act, new_action) {
        return false;
    }
    match last {
        None => !(new_action == Action::Request && acting_user == new_user),
        Some(last) => {
            if last.acting_user == acting_user {
                !matches!(new_action, Action::Accept | Action::Reject)
            } else {
                new_action != Action::Cancel
            }
        }
    }
}

pub(super) fn friendship_action_status(
    last: &crate::rpc::db::LastAction,
    me: &str,
) -> Option<FriendshipStatus> {
    let acting_is_me = last.acting_user == me;
    Some(match Action::from_str(&last.action)? {
        Action::Accept => FriendshipStatus::Accepted,
        Action::Cancel => FriendshipStatus::Canceled,
        Action::Delete => FriendshipStatus::Deleted,
        Action::Reject => FriendshipStatus::Rejected,
        Action::Request if acting_is_me => FriendshipStatus::RequestSent,
        Action::Request => FriendshipStatus::RequestReceived,
        Action::Block if acting_is_me => FriendshipStatus::Blocked,
        Action::Block => FriendshipStatus::BlockedBy,
    })
}

pub(super) fn status_ok(status: FriendshipStatus) -> GetFriendshipStatusResponse {
    GetFriendshipStatusResponse {
        response: Some(get_friendship_status_response::Response::Accepted(
            get_friendship_status_response::Ok {
                status: status as i32,
                message: None,
            },
        )),
    }
}

pub(super) fn settings_to_proto(row: &crate::rpc::db::SocialSettingsRow) -> SocialSettings {
    SocialSettings {
        private_messages_privacy: pmp_from_db(&row.private_messages_privacy) as i32,
        blocked_users_messages_visibility: bvis_from_db(&row.blocked_users_messages_visibility)
            as i32,
        show_situation_reactions: sreact_from_db(&row.show_situation_reactions) as i32,
    }
}

pub(super) fn pmp_to_db(v: PrivateMessagePrivacySetting) -> String {
    match v {
        PrivateMessagePrivacySetting::All => "all",
        PrivateMessagePrivacySetting::OnlyFriends => "only_friends",
    }
    .to_string()
}
pub(super) fn pmp_from_db(s: &str) -> PrivateMessagePrivacySetting {
    match s {
        "all" => PrivateMessagePrivacySetting::All,
        _ => PrivateMessagePrivacySetting::OnlyFriends,
    }
}

pub(super) fn bvis_to_db(v: BlockedUsersMessagesVisibilitySetting) -> String {
    match v {
        BlockedUsersMessagesVisibilitySetting::ShowMessages => "show_messages",
        BlockedUsersMessagesVisibilitySetting::DoNotShowMessages => "do_not_show_messages",
    }
    .to_string()
}
fn bvis_from_db(s: &str) -> BlockedUsersMessagesVisibilitySetting {
    match s {
        "do_not_show_messages" => BlockedUsersMessagesVisibilitySetting::DoNotShowMessages,
        _ => BlockedUsersMessagesVisibilitySetting::ShowMessages,
    }
}

pub(super) fn sreact_to_db(v: SituationReactionsVisibility) -> String {
    match v {
        SituationReactionsVisibility::Show => "show",
        SituationReactionsVisibility::Hide => "hide",
    }
    .to_string()
}
fn sreact_from_db(s: &str) -> SituationReactionsVisibility {
    match s {
        "hide" => SituationReactionsVisibility::Hide,
        _ => SituationReactionsVisibility::Show,
    }
}

pub(super) fn friendship_update_for(
    action: Action,
    from: &str,
    id: &Uuid,
    created_ms: i64,
    message: Option<&str>,
    from_profile: FriendProfile,
) -> FriendshipUpdate {
    let user = Some(User {
        address: from.to_string(),
    });
    let update = match action {
        Action::Request => friendship_update::Update::Request(friendship_update::RequestResponse {
            friend: Some(from_profile),
            created_at: created_ms,
            message: message.map(|m| m.to_string()),
            id: id.to_string(),
        }),
        Action::Accept => {
            friendship_update::Update::Accept(friendship_update::AcceptResponse { user })
        }
        Action::Reject => {
            friendship_update::Update::Reject(friendship_update::RejectResponse { user })
        }
        Action::Cancel => {
            friendship_update::Update::Cancel(friendship_update::CancelResponse { user })
        }
        Action::Delete => {
            friendship_update::Update::Delete(friendship_update::DeleteResponse { user })
        }
        Action::Block => {
            friendship_update::Update::Block(friendship_update::BlockResponse { user })
        }
    };
    FriendshipUpdate {
        update: Some(update),
    }
}

pub(super) async fn friendship_requests(
    context: &ProcedureContext<Context>,
    request: GetFriendshipRequestsPayload,
    incoming: bool,
) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
    let me = SocialServiceImpl::caller(context)?;
    let db = context.server_context.db();
    let (limit, offset) = page_friendship_requests(&request.pagination);
    let fetched = async {
        let rows = db
            .get_friendship_requests(&me, incoming, limit, offset)
            .await?;
        let total = db.count_friendship_requests(&me, incoming).await?;
        Ok::<_, crate::rpc::db::DbError>((rows, total))
    }
    .await;
    let (rows, total) = match fetched {
        Ok(v) => v,
        Err(_) => {
            return Ok(PaginatedFriendshipRequestsResponse {
                response: Some(
                    paginated_friendship_requests_response::Response::InternalServerError(
                        InternalServerError { message: None },
                    ),
                ),
                pagination_data: None,
            })
        }
    };
    let addrs: Vec<String> = rows.iter().map(|r| r.address.clone()).collect();
    let map = context.server_context.profiles().get_profiles(&addrs).await;
    let requests = rows
        .into_iter()
        .map(|r| {
            let friend = match map.get(&r.address.to_lowercase()) {
                Some(info) => FriendProfile {
                    address: r.address.clone(),
                    name: info.name.clone(),
                    has_claimed_name: info.has_claimed_name,
                    profile_picture_url: info.profile_picture_url.clone(),
                    name_color: info.name_color.clone(),
                },
                None => FriendProfile {
                    address: r.address.clone(),
                    name: String::new(),
                    has_claimed_name: false,
                    profile_picture_url: String::new(),
                    name_color: None,
                },
            };
            FriendshipRequestResponse {
                friend: Some(friend),
                created_at: r.timestamp.timestamp_millis(),
                message: r.message,
                id: r.id.to_string(),
            }
        })
        .collect();
    Ok(PaginatedFriendshipRequestsResponse {
        response: Some(paginated_friendship_requests_response::Response::Requests(
            FriendshipRequests { requests },
        )),
        pagination_data: Some(PaginatedResponse {
            total: total as i32,
            page: page_of(limit, offset),
        }),
    })
}

pub(super) async fn fan_community_voice(
    ctx: &Context,
    community_id: &str,
    status: CommunityVoiceChatStatus,
    exclude: Option<&str>,
) {
    let db = ctx.db();
    let community_name = db
        .community_name(community_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let members = db
        .community_member_addresses(community_id)
        .await
        .unwrap_or_default();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let ended_at =
        matches!(status, CommunityVoiceChatStatus::CommunityVoiceChatEnded).then_some(now_ms);
    for member in &members {
        if exclude.is_some_and(|e| e.eq_ignore_ascii_case(member)) {
            continue;
        }
        ctx.pubsub().publish(
            member,
            SocialEvent::CommunityVoice(CommunityVoiceChatUpdate {
                community_id: community_id.to_string(),
                created_at: now_ms,
                status: status as i32,
                ended_at,
                positions: Vec::new(),
                is_member: true,
                community_name: community_name.clone(),
                community_image: None,
                worlds: Vec::new(),
            }),
        );
    }
}

/// Whether one wallet may moderate one community's voice room, decided from the same
/// `community_members` row the REST paths read.
///
/// **Deliberate behaviour change (BC-4).** This was the sixth and last independent role
/// vocabulary in this crate, and the only one spelled `role == "owner" || role ==
/// "moderator"`. It therefore refused a wallet whose stored role is the literal `"mod"` --
/// a value `rest::fed::apply`'s role projection writes into `community_members` for a
/// federated moderator (`apply_role` copies `community_role_current.role`, whose canonical
/// moderator spelling is `"mod"`). A federated moderator could ban from the REST API and
/// not moderate a voice room. It now reaches the shared tier parse and the shared
/// capability matrix, so the two agree.
pub(super) async fn require_moderator(
    db: &Db,
    community_id: &str,
    address: &str,
) -> Result<Result<(), ForbiddenError>, SocialError> {
    use crate::rest::community_membership_authority::CommunityMembershipTier;
    use crate::rest::handlers::permissions::Permission;

    let tier = match db.community_role(community_id, address).await? {
        Some(stored_role_text) => {
            CommunityMembershipTier::parse_role_text_as_stored_in_a_table(&stored_role_text)
        }
        None => CommunityMembershipTier::NotAMemberOfThisCommunity,
    };
    if tier.holds_capability_within_this_community(Permission::BanPlayers) {
        Ok(Ok(()))
    } else {
        Ok(Err(ForbiddenError {
            message: Some("requires moderator or owner role".into()),
        }))
    }
}

/// Reads one wallet's tier in one community from the same `community_members` row the REST
/// paths read. An absent or unrecognised value is [`CommunityMembershipTier::NotAMemberOfThisCommunity`].
async fn community_tier(
    db: &Db,
    community_id: &str,
    address: &str,
) -> Result<crate::rest::community_membership_authority::CommunityMembershipTier, SocialError> {
    use crate::rest::community_membership_authority::CommunityMembershipTier;
    Ok(match db.community_role(community_id, address).await? {
        Some(text) => CommunityMembershipTier::parse_role_text_as_stored_in_a_table(&text),
        None => CommunityMembershipTier::NotAMemberOfThisCommunity,
    })
}

/// Privacy-aware participation gate for the self-service community-voice actions -- raising a hand
/// (`request_to_speak`) and self-unmute. Mirrors the reachable half of upstream
/// `validateCommunityVoiceChatParticipation` (social-service-ea #447).
///
/// Enforces the two rules this crate's data model can decide locally: the actor must not be banned,
/// and a **private** community requires membership while a **public** community admits a guest who
/// holds no role. In this crate a ban is a `community_members.role = 'banned'` row, so it surfaces
/// as [`CommunityMembershipTier::BannedFromThisCommunity`] through the shared tier parse.
///
/// **OWED divergence:** the room-live check (upstream `getCommunityVoiceChatStatus(communityId)`)
/// is not ported. Our gatekeeper client exposes only a per-user `is_user_in_community_voice_chat`,
/// not a per-community room-status endpoint, and this pass does not invent a new external client.
/// A missing/inactive room is therefore not rejected here.
pub(super) async fn validate_community_voice_participation(
    db: &Db,
    community_id: &str,
    address: &str,
) -> Result<Result<(), ForbiddenError>, SocialError> {
    use crate::rest::community_membership_authority::CommunityMembershipTier;

    let tier = community_tier(db, community_id, address).await?;
    if tier == CommunityMembershipTier::BannedFromThisCommunity {
        return Ok(Err(ForbiddenError {
            message: Some("banned from this community".into()),
        }));
    }
    // Absent privacy (community row gone) defaults to public: there is nothing to protect, and the
    // owed room-live gate is what would reject a non-existent room.
    let private = db
        .community_is_private(community_id)
        .await?
        .unwrap_or(false);
    if private && tier == CommunityMembershipTier::NotAMemberOfThisCommunity {
        return Ok(Err(ForbiddenError {
            message: Some("not a community member".into()),
        }));
    }
    Ok(Ok(()))
}

/// Privacy-aware target-membership gate for the community-voice moderation actions that grant or
/// move a capability (promote/demote/reject), mirroring upstream
/// `validateCommunityVoiceChatTargetMembership` (#447). A public community admits any target the
/// live room already holds -- comms-gatekeeper owns presence -- while a private community requires
/// the target to be a member. Called only after the actor has cleared the moderator/owner gate.
pub(super) async fn validate_community_voice_target_membership(
    db: &Db,
    community_id: &str,
    target: &str,
) -> Result<Result<(), ForbiddenError>, SocialError> {
    use crate::rest::community_membership_authority::CommunityMembershipTier;

    let private = db
        .community_is_private(community_id)
        .await?
        .unwrap_or(false);
    if private {
        let tier = community_tier(db, community_id, target).await?;
        if tier == CommunityMembershipTier::NotAMemberOfThisCommunity {
            return Ok(Err(ForbiddenError {
                message: Some("not a community member".into()),
            }));
        }
    }
    Ok(Ok(()))
}

/// Like [`require_moderator`], but additionally protects the community owner: a voice-room
/// moderation aimed at the owner is refused unless the actor **is** the owner, mirroring
/// upstream's `validateCommunityVoiceChatModerator` (social-service-ea #447). A self-action
/// still bypasses only the owner-protection clause -- the capability gate always applies, so a
/// non-moderator cannot promote/kick/reject even themselves.
///
/// `action` is the human-readable verb woven into the refusal message, matching upstream
/// ("promote speakers", "kick players", ...).
pub(super) async fn require_moderator_protecting_owner(
    db: &Db,
    community_id: &str,
    actor: &str,
    target: &str,
    action: &str,
) -> Result<Result<(), ForbiddenError>, SocialError> {
    if let Err(f) = require_moderator(db, community_id, actor).await? {
        return Ok(Err(f));
    }

    let is_self_action = actor.trim().to_lowercase() == target.trim().to_lowercase();
    if is_self_action {
        return Ok(Ok(()));
    }

    let actor_tier = community_tier(db, community_id, actor).await?;
    let target_tier = community_tier(db, community_id, target).await?;
    if targeting_the_owner_as_a_non_owner(actor_tier, target_tier) {
        return Ok(Err(ForbiddenError {
            message: Some(format!("Not enough permissions to {} this user", action)),
        }));
    }
    Ok(Ok(()))
}

/// The owner-protection predicate, factored out for testing: only the owner may be acted on,
/// and only by the owner. Peer moderators may still moderate each other, since these actions
/// are confined to the live room and are reversible. Callers exempt self-actions before this.
fn targeting_the_owner_as_a_non_owner(
    actor_tier: crate::rest::community_membership_authority::CommunityMembershipTier,
    target_tier: crate::rest::community_membership_authority::CommunityMembershipTier,
) -> bool {
    use crate::rest::community_membership_authority::CommunityMembershipTier;
    target_tier == CommunityMembershipTier::OwnerOfThisCommunity
        && actor_tier != CommunityMembershipTier::OwnerOfThisCommunity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::db::LastAction;
    use uuid::Uuid;

    fn last(action: &str, acting_user: &str) -> LastAction {
        LastAction {
            friendship_id: Uuid::nil(),
            action: action.to_string(),
            acting_user: acting_user.to_string(),
            is_active: false,
        }
    }

    fn resolve(
        last: Option<&LastAction>,
        me: &str,
        me_blocked_other: bool,
        other_blocked_me: bool,
    ) -> FriendshipStatus {
        match last.and_then(|l| friendship_action_status(l, me)) {
            Some(s) => s,
            None => {
                if me_blocked_other {
                    FriendshipStatus::Blocked
                } else if other_blocked_me {
                    FriendshipStatus::BlockedBy
                } else {
                    FriendshipStatus::None
                }
            }
        }
    }

    const ME: &str = "0xme";
    const OTHER: &str = "0xother";

    #[test]
    fn action_mapping_covers_every_action_and_direction() {
        assert_eq!(
            friendship_action_status(&last("accept", OTHER), ME),
            Some(FriendshipStatus::Accepted)
        );
        assert_eq!(
            friendship_action_status(&last("cancel", ME), ME),
            Some(FriendshipStatus::Canceled)
        );
        assert_eq!(
            friendship_action_status(&last("delete", OTHER), ME),
            Some(FriendshipStatus::Deleted)
        );
        assert_eq!(
            friendship_action_status(&last("reject", OTHER), ME),
            Some(FriendshipStatus::Rejected)
        );

        assert_eq!(
            friendship_action_status(&last("request", ME), ME),
            Some(FriendshipStatus::RequestSent)
        );
        assert_eq!(
            friendship_action_status(&last("request", OTHER), ME),
            Some(FriendshipStatus::RequestReceived)
        );

        assert_eq!(
            friendship_action_status(&last("block", ME), ME),
            Some(FriendshipStatus::Blocked)
        );
        assert_eq!(
            friendship_action_status(&last("block", OTHER), ME),
            Some(FriendshipStatus::BlockedBy)
        );

        assert_eq!(friendship_action_status(&last("", ME), ME), None);
        assert_eq!(friendship_action_status(&last("garbage", ME), ME), None);
    }

    #[test]
    fn friendship_action_takes_precedence_over_blocks() {
        assert_eq!(
            resolve(Some(&last("accept", OTHER)), ME, true, true),
            FriendshipStatus::Accepted
        );
        assert_eq!(
            resolve(Some(&last("request", ME)), ME, true, false),
            FriendshipStatus::RequestSent
        );

        assert_eq!(
            resolve(Some(&last("block", OTHER)), ME, false, true),
            FriendshipStatus::BlockedBy
        );
    }

    #[test]
    fn block_fallback_only_when_no_friendship_action() {
        assert_eq!(resolve(None, ME, true, false), FriendshipStatus::Blocked);
        assert_eq!(resolve(None, ME, false, true), FriendshipStatus::BlockedBy);
        assert_eq!(resolve(None, ME, false, false), FriendshipStatus::None);

        assert_eq!(resolve(None, ME, true, true), FriendshipStatus::Blocked);
    }

    #[test]
    fn no_prior_action_only_admits_request_and_block() {
        assert!(user_action_valid(ME, Action::Request, OTHER, None));
        assert!(user_action_valid(ME, Action::Block, OTHER, None));

        assert!(!user_action_valid(ME, Action::Accept, OTHER, None));
        assert!(!user_action_valid(ME, Action::Reject, OTHER, None));
        assert!(!user_action_valid(ME, Action::Delete, OTHER, None));
        assert!(!user_action_valid(ME, Action::Cancel, OTHER, None));
    }

    #[test]
    fn self_request_is_rejected_even_without_prior_action() {
        assert!(!user_action_valid(ME, Action::Request, ME, None));
    }

    #[test]
    fn full_transition_table_is_enforced() {
        let all = [
            Action::Request,
            Action::Accept,
            Action::Cancel,
            Action::Reject,
            Action::Delete,
            Action::Block,
        ];
        for from in all {
            for to in all {
                let legal = match to {
                    Action::Request => {
                        matches!(from, Action::Cancel | Action::Reject | Action::Delete)
                    }
                    Action::Accept | Action::Cancel | Action::Reject => from == Action::Request,
                    Action::Delete => matches!(from, Action::Accept | Action::Block),
                    Action::Block => from != Action::Block,
                };
                assert_eq!(
                    transition_valid(Some(from), to),
                    legal,
                    "transition {:?} -> {:?}",
                    from,
                    to
                );
            }
        }
    }

    #[test]
    fn acting_user_rules_still_apply_after_transition_check() {
        let sent = last("request", ME);
        assert!(!user_action_valid(ME, Action::Accept, OTHER, Some(&sent)));
        assert!(!user_action_valid(ME, Action::Reject, OTHER, Some(&sent)));
        assert!(user_action_valid(ME, Action::Cancel, OTHER, Some(&sent)));

        let received = last("request", OTHER);
        assert!(user_action_valid(
            ME,
            Action::Accept,
            OTHER,
            Some(&received)
        ));
        assert!(user_action_valid(
            ME,
            Action::Reject,
            OTHER,
            Some(&received)
        ));
        assert!(!user_action_valid(
            ME,
            Action::Cancel,
            OTHER,
            Some(&received)
        ));

        let accepted = last("accept", OTHER);
        assert!(!user_action_valid(
            ME,
            Action::Accept,
            OTHER,
            Some(&accepted)
        ));
        assert!(user_action_valid(
            ME,
            Action::Delete,
            OTHER,
            Some(&accepted)
        ));
    }

    #[test]
    fn friendship_row_without_action_falls_through_to_blocks() {
        assert_eq!(
            resolve(Some(&last("", OTHER)), ME, false, true),
            FriendshipStatus::BlockedBy
        );
        assert_eq!(
            resolve(Some(&last("", ME)), ME, true, false),
            FriendshipStatus::Blocked
        );

        assert_eq!(
            resolve(Some(&last("", ME)), ME, false, false),
            FriendshipStatus::None
        );
    }

    #[test]
    fn owner_protection_only_bites_a_non_owner_targeting_the_owner() {
        use crate::rest::community_membership_authority::CommunityMembershipTier as T;

        // A moderator may not act on the owner.
        assert!(targeting_the_owner_as_a_non_owner(
            T::ModeratorOfThisCommunity,
            T::OwnerOfThisCommunity
        ));
        // Neither may a non-member (the capability gate catches this first, but the predicate
        // must still refuse it).
        assert!(targeting_the_owner_as_a_non_owner(
            T::NotAMemberOfThisCommunity,
            T::OwnerOfThisCommunity
        ));
        // The owner may act on the owner (the self-action exemption is applied by the caller,
        // and an owner acting on an owner is only ever a self-action).
        assert!(!targeting_the_owner_as_a_non_owner(
            T::OwnerOfThisCommunity,
            T::OwnerOfThisCommunity
        ));
        // Peer moderators may moderate each other, and anyone privileged may act on a member.
        assert!(!targeting_the_owner_as_a_non_owner(
            T::ModeratorOfThisCommunity,
            T::ModeratorOfThisCommunity
        ));
        assert!(!targeting_the_owner_as_a_non_owner(
            T::ModeratorOfThisCommunity,
            T::OrdinaryMemberOfThisCommunity
        ));
    }
}
