//! `friendshipStatus` derivation for community member listings.
//!
//! Port of social-service-ea's `latest_friendship_actions` CTE
//! (`logic/queries.ts:getLatestFriendshipActionCTE`) + the
//! `getFriendshipRequestStatus` mapping (`logic/friends/friendships.ts:223`),
//! as consumed by `mapMembersWithProfiles` (`logic/community/utils.ts:128`).
//!
//! For each member, upstream looks up the latest action on the friendship
//! between the requesting user and that member, then maps the `(action,
//! acting_user)` pair to the protobuf `FriendshipStatus` enum (numeric on the
//! wire). When the requester is anonymous, or no friendship row exists, the
//! status is `NONE` (7).
//!
//! The `friendships` / `friendship_actions` tables live in the same shared
//! `communities` database (see catalyrst-social-rpc migration 0001), so this is
//! a real local join, not an approximation.

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

/// Protobuf `FriendshipStatus` enum
/// (decentraland/social_service/v2/social_service_v2.proto:147). Serialized as
/// its numeric value on the wire, exactly as upstream emits it.
pub mod friendship_status {
    pub const REQUEST_SENT: i32 = 0;
    pub const REQUEST_RECEIVED: i32 = 1;
    pub const CANCELED: i32 = 2;
    pub const ACCEPTED: i32 = 3;
    pub const REJECTED: i32 = 4;
    pub const DELETED: i32 = 5;
    pub const BLOCKED: i32 = 6;
    pub const NONE: i32 = 7;
    pub const BLOCKED_BY: i32 = 8;
    /// protobuf `UNRECOGNIZED` sentinel, returned for an unknown action string.
    pub const UNRECOGNIZED: i32 = -1;
}

/// Port of `getFriendshipRequestStatus` + `FRIENDSHIP_STATUS_BY_ACTION`
/// (friendships.ts:24-36). `acting_user` and `context_address` are compared
/// lowercased (upstream normalizes both before storing/comparing).
pub fn friendship_request_status(action: &str, acting_user: &str, context_address: &str) -> i32 {
    use friendship_status::*;
    let acting_is_context = acting_user.eq_ignore_ascii_case(context_address);
    match action {
        "accept" => ACCEPTED,
        "cancel" => CANCELED,
        "delete" => DELETED,
        "reject" => REJECTED,
        "request" => {
            if acting_is_context {
                REQUEST_SENT
            } else {
                REQUEST_RECEIVED
            }
        }
        "block" => {
            if acting_is_context {
                BLOCKED
            } else {
                BLOCKED_BY
            }
        }
        _ => UNRECOGNIZED,
    }
}

/// Result of the latest-friendship-action lookup for a single counterparty:
/// the most recent `(action, acting_user)` on the friendship between the
/// requesting user and `other_user`.
struct LatestAction {
    action: String,
    acting_user: String,
}

/// Build the `other_user -> FriendshipStatus` map for `member_addresses` as seen
/// by `user_address`, mirroring the `getCommunityMembers` LEFT JOIN onto
/// `latest_friendship_actions`. Members with no friendship row are absent from
/// the returned map and default to `NONE` at the call site.
///
/// `user_address` and `member_addresses` are expected already lowercased.
pub async fn friendship_statuses(
    pool: &PgPool,
    user_address: &str,
    member_addresses: &[String],
) -> HashMap<String, i32> {
    let mut out = HashMap::new();
    if member_addresses.is_empty() {
        return out;
    }

    // Port of getLatestFriendshipActionCTE: DISTINCT ON (f.id) keeps the latest
    // action per friendship (ORDER BY f.id, fa.timestamp DESC), and `other_user`
    // is the counterparty relative to the requesting user. We bind the candidate
    // member set explicitly (`= ANY`) rather than joining a `members` CTE; the
    // result is identical because the outer query only consumes the rows whose
    // `other_user` is one of those members.
    let rows: Vec<(String, String, String)> = match sqlx::query_as(
        "SELECT DISTINCT ON (f.id) \
           CASE WHEN f.address_requester = $1 THEN f.address_requested \
                ELSE f.address_requester END AS other_user, \
           fa.action, \
           fa.acting_user \
         FROM friendships f \
         INNER JOIN friendship_actions fa ON f.id = fa.friendship_id \
         WHERE (f.address_requester = $1 AND f.address_requested = ANY($2)) \
            OR (f.address_requested = $1 AND f.address_requester = ANY($2)) \
         ORDER BY f.id, fa.timestamp DESC",
    )
    .bind(user_address)
    .bind(member_addresses)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            // A missing friendships table (deployment without the social schema)
            // degrades to all-NONE rather than failing the members listing.
            tracing::warn!(error = %e, "friendship status lookup failed; defaulting members to NONE");
            return out;
        }
    };

    for (other_user, action, acting_user) in rows {
        let latest = LatestAction {
            action,
            acting_user,
        };
        let status = friendship_request_status(&latest.action, &latest.acting_user, user_address);
        out.insert(other_user.to_lowercase(), status);
    }
    out
}

/// Build the `community_id -> [friend_address]` map used to populate each
/// community's `friends` array in `GET /v1/communities`. Port of the
/// `community_friends` CTE in upstream `communities-db.ts:getCommunities`: for
/// each community, the requesting user's active friends who are also members,
/// ordered by address and capped at 3.
///
/// `user_address` is expected already lowercased. Communities with no shared
/// friends are absent from the map (the caller emits `[]`). A missing
/// `friendships` table degrades to an empty map rather than failing the listing.
pub async fn community_friends(
    pool: &PgPool,
    user_address: &str,
    community_ids: &[Uuid],
) -> HashMap<Uuid, Vec<String>> {
    let mut out: HashMap<Uuid, Vec<String>> = HashMap::new();
    if community_ids.is_empty() {
        return out;
    }

    let rows: Vec<(Uuid, String)> = match sqlx::query_as(
        "SELECT community_id, address FROM ( \
           SELECT cm.community_id, uf.address, \
                  ROW_NUMBER() OVER (PARTITION BY cm.community_id ORDER BY uf.address) AS rn \
           FROM ( \
             SELECT DISTINCT CASE WHEN f.address_requester = $1 \
                                    THEN f.address_requested \
                                    ELSE f.address_requester END AS address \
             FROM friendships f \
             WHERE f.is_active = TRUE \
               AND (f.address_requester = $1 OR f.address_requested = $1) \
           ) uf \
           JOIN community_members cm ON cm.member_address = uf.address \
           WHERE cm.community_id = ANY($2) \
         ) ranked \
         WHERE rn <= 3",
    )
    .bind(user_address)
    .bind(community_ids)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "community friends lookup failed; defaulting to no friends");
            return out;
        }
    };

    for (community_id, address) in rows {
        out.entry(community_id).or_default().push(address);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::friendship_request_status;
    use super::friendship_status::*;

    const ME: &str = "0xaaa";
    const OTHER: &str = "0xbbb";

    #[test]
    fn request_sent_when_acting_user_is_context() {
        assert_eq!(friendship_request_status("request", ME, ME), REQUEST_SENT);
    }

    #[test]
    fn request_received_when_other_acts() {
        assert_eq!(
            friendship_request_status("request", OTHER, ME),
            REQUEST_RECEIVED
        );
    }

    #[test]
    fn blocked_vs_blocked_by() {
        assert_eq!(friendship_request_status("block", ME, ME), BLOCKED);
        assert_eq!(friendship_request_status("block", OTHER, ME), BLOCKED_BY);
    }

    #[test]
    fn terminal_actions_ignore_acting_user() {
        assert_eq!(friendship_request_status("accept", OTHER, ME), ACCEPTED);
        assert_eq!(friendship_request_status("cancel", OTHER, ME), CANCELED);
        assert_eq!(friendship_request_status("delete", OTHER, ME), DELETED);
        assert_eq!(friendship_request_status("reject", OTHER, ME), REJECTED);
    }

    #[test]
    fn case_insensitive_acting_user_match() {
        // Upstream normalizes addresses; an uppercase acting_user equal to the
        // context still resolves to REQUEST_SENT / BLOCKED.
        assert_eq!(
            friendship_request_status("request", "0xAAA", ME),
            REQUEST_SENT
        );
        assert_eq!(friendship_request_status("block", "0xAAA", ME), BLOCKED);
    }

    #[test]
    fn unknown_action_is_unrecognized() {
        assert_eq!(
            friendship_request_status("frobnicate", ME, ME),
            UNRECOGNIZED
        );
    }

    #[test]
    fn none_enum_value_is_seven() {
        // The default emitted on members with no friendship row.
        assert_eq!(NONE, 7);
    }
}
