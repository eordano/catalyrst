use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

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

    pub const UNRECOGNIZED: i32 = -1;
}

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

struct LatestAction {
    action: String,
    acting_user: String,
}

pub async fn friendship_statuses(
    pool: &PgPool,
    user_address: &str,
    member_addresses: &[String],
) -> HashMap<String, i32> {
    let mut out = HashMap::new();
    if member_addresses.is_empty() {
        return out;
    }

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

pub async fn community_friends(
    social_pool: &PgPool,
    communities_pool: &PgPool,
    user_address: &str,
    community_ids: &[Uuid],
) -> HashMap<Uuid, Vec<String>> {
    let mut out: HashMap<Uuid, Vec<String>> = HashMap::new();
    if community_ids.is_empty() {
        return out;
    }
    let user_address = user_address.to_lowercase();

    let friend_rows: Vec<(String,)> = match sqlx::query_as(
        "SELECT DISTINCT CASE WHEN f.address_requester = $1 \
                                THEN f.address_requested \
                                ELSE f.address_requester END AS address \
         FROM friendships f \
         WHERE f.is_active = TRUE \
           AND (f.address_requester = $1 OR f.address_requested = $1)",
    )
    .bind(&user_address)
    .fetch_all(social_pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "community friends: friend lookup failed; defaulting to no friends");
            return out;
        }
    };
    let friends: Vec<String> = friend_rows.into_iter().map(|(a,)| a).collect();
    if friends.is_empty() {
        return out;
    }

    let rows: Vec<(Uuid, String)> = match sqlx::query_as(
        "SELECT community_id, member_address FROM ( \
           SELECT cm.community_id, cm.member_address, \
                  ROW_NUMBER() OVER (PARTITION BY cm.community_id ORDER BY cm.member_address) AS rn \
           FROM community_members cm \
           WHERE cm.community_id = ANY($1) AND cm.member_address = ANY($2) \
         ) ranked \
         WHERE rn <= 3",
    )
    .bind(community_ids)
    .bind(&friends)
    .fetch_all(communities_pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "community friends: membership lookup failed; defaulting to no friends");
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
        assert_eq!(NONE, 7);
    }
}
