use crate::rpc::proto::errors::*;
use crate::rpc::proto::v2::*;
use crate::rpc::pubsub::{PubSub, SocialEvent};
use catalyrst_drpc::rpc_protocol::RemoteErrorResponse;
use catalyrst_drpc::stream_protocol::Generator;
use tokio::sync::broadcast::error::RecvError;

// Per-endpoint pagination bounds, mirroring upstream `src/utils/friendship-pagination.ts` (#455).
// Each maximum is set at or above the largest page a shipping client actually requests: clients
// derive their next offset from the page size they ASKED for, not the rows returned, so a cap
// below that would silently truncate their list rather than make them re-page.
//
// Unity Explorer pages friendship requests at 100; 200 leaves headroom for a client bump.
const FRIENDSHIP_REQUESTS_DEFAULT_LIMIT: i64 = 100;
const FRIENDSHIP_REQUESTS_MAX_LIMIT: i64 = 200;
// Unity Explorer's friends-cache prewarm asks for 1000 in one shot; godot-explorer asks for 1000
// mutual friends. Capping either lower truncates those lists silently, so friends and mutual must
// NOT share the generic 100 cap the other reads used.
const FRIENDS_DEFAULT_LIMIT: i64 = 1000;
const FRIENDS_MAX_LIMIT: i64 = 1000;
// Only Unity Explorer reads the blocklist, at a page size of 50.
const BLOCKED_USERS_DEFAULT_LIMIT: i64 = 200;
const BLOCKED_USERS_MAX_LIMIT: i64 = 200;
// No client can reach this: every one bounds its offset by the total the server reports.
const MAX_PAGINATION_OFFSET: i64 = 100_000;

// Client-visible exhaustion message for the friendship/block mutation throttle, matching upstream
// FriendshipRateLimitError (#456).
pub(super) const FRIENDSHIP_RATE_LIMIT_MESSAGE: &str =
    "Too many friendship or block actions. Please try again later";

#[derive(Debug, thiserror::Error)]
pub enum SocialError {
    #[error("internal server error: {0}")]
    Internal(String),
    #[error("not authenticated")]
    Unauthenticated,
}

impl RemoteErrorResponse for SocialError {
    fn error_code(&self) -> u32 {
        match self {
            SocialError::Internal(_) => 500,
            SocialError::Unauthenticated => 401,
        }
    }
    fn error_message(&self) -> String {
        self.to_string()
    }
}

impl From<crate::rpc::db::DbError> for SocialError {
    fn from(e: crate::rpc::db::DbError) -> Self {
        SocialError::Internal(e.to_string())
    }
}

pub(super) fn normalize(addr: &str) -> String {
    addr.trim().to_lowercase()
}

/// Bounded `(limit, offset)` for a list read, mirroring upstream `normalizePagination(bounds)`.
///
/// A missing, zero or negative page size falls back to `default_limit`; a larger one is clamped to
/// `max_limit`; the offset is clamped to [`MAX_PAGINATION_OFFSET`]. The proto carries `limit`/
/// `offset` as `i32`, so there are no fractional cases to floor (unlike the TS original).
///
/// The caps are silent: `PaginatedResponse` carries only `total` and `page`, so a capped caller
/// cannot tell. That is why each maximum is set at or above the largest page a shipping client
/// requests -- see the bounds constants above. A client raising its page size past a cap needs the
/// constant raised in the same change.
pub(super) fn page_bounded(
    p: &Option<Pagination>,
    default_limit: i64,
    max_limit: i64,
) -> (i64, i64) {
    match p {
        Some(p) => {
            let limit = if p.limit < 1 {
                default_limit
            } else {
                (p.limit as i64).min(max_limit)
            };
            let offset = if p.offset < 0 {
                0
            } else {
                (p.offset as i64).min(MAX_PAGINATION_OFFSET)
            };
            (limit, offset)
        }
        None => (default_limit, 0),
    }
}

/// Bounded pagination for the friends and mutual-friends reads (default = max = 1000).
pub(super) fn page_friends(p: &Option<Pagination>) -> (i64, i64) {
    page_bounded(p, FRIENDS_DEFAULT_LIMIT, FRIENDS_MAX_LIMIT)
}

/// Bounded pagination for the pending/sent friendship-request reads (default 100, max 200).
pub(super) fn page_friendship_requests(p: &Option<Pagination>) -> (i64, i64) {
    page_bounded(
        p,
        FRIENDSHIP_REQUESTS_DEFAULT_LIMIT,
        FRIENDSHIP_REQUESTS_MAX_LIMIT,
    )
}

/// Bounded pagination for the blocked-users read (default = max = 200).
pub(super) fn page_blocked_users(p: &Option<Pagination>) -> (i64, i64) {
    page_bounded(p, BLOCKED_USERS_DEFAULT_LIMIT, BLOCKED_USERS_MAX_LIMIT)
}

/// The 1-based page number for an already-bounded `(limit, offset)`, mirroring upstream `getPage`.
/// Callers pass the same bounded values they handed to the query, so the reported page matches the
/// page actually fetched.
pub(super) fn page_of(limit: i64, offset: i64) -> i32 {
    if limit <= 0 {
        return 1;
    }
    let off = offset.max(0);
    (((off as f64) / (limit as f64)).ceil() as i64 + 1) as i32
}

pub(super) fn is_eth_address(addr: &str) -> bool {
    catalyrst_types::is_eth_address(addr)
}

pub(super) fn empty_friends_profiles() -> PaginatedFriendsProfilesResponse {
    PaginatedFriendsProfilesResponse {
        friends: Vec::new(),
        pagination_data: Some(PaginatedResponse { total: 0, page: 1 }),
    }
}

pub(super) fn friendship_status_invalid(msg: impl Into<String>) -> GetFriendshipStatusResponse {
    GetFriendshipStatusResponse {
        response: Some(get_friendship_status_response::Response::InvalidRequest(
            invalid_req(msg),
        )),
    }
}

pub(super) fn upsert_internal_error(msg: impl Into<String>) -> UpsertFriendshipResponse {
    UpsertFriendshipResponse {
        response: Some(upsert_friendship_response::Response::InternalServerError(
            internal_err(msg),
        )),
    }
}

pub(super) fn internal_err(msg: impl Into<String>) -> InternalServerError {
    let detail = msg.into();
    tracing::error!(detail = %detail, "social-rpc internal error");
    InternalServerError {
        message: Some("internal error".to_string()),
    }
}

pub(super) fn invalid_req(msg: impl Into<String>) -> InvalidRequest {
    InvalidRequest {
        message: Some(msg.into()),
    }
}

pub(super) fn start_voice_invalid(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(start_private_voice_chat_response::Response::InvalidRequest(
            invalid_req(msg),
        )),
    }
}

pub(super) fn start_voice_conflict(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(
            start_private_voice_chat_response::Response::ConflictingError(ConflictingError {
                message: Some(msg.into()),
            }),
        ),
    }
}

pub(super) fn start_voice_forbidden(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(start_private_voice_chat_response::Response::ForbiddenError(
            ForbiddenError {
                message: Some(msg.into()),
            },
        )),
    }
}

pub(super) fn start_voice_internal(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(
            start_private_voice_chat_response::Response::InternalServerError(internal_err(msg)),
        ),
    }
}

pub(super) fn stream_for<T, F>(pubsub: &PubSub, address: &str, pick: F) -> Generator<T>
where
    T: Send + Sync + 'static,
    F: Fn(&SocialEvent) -> Option<T> + Send + 'static,
{
    let (generator, yielder) = Generator::create();
    let mut rx = pubsub.subscribe(address);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                // `event` is an `Arc<SocialEvent>`, so the picker borrows and clones only the
                // matched inner variant.
                Ok(event) => {
                    if let Some(item) = pick(&event) {
                        if yielder.r#yield(item).await.is_err() {
                            break;
                        }
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "subscription stream lagged; events dropped");
                    continue;
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
    generator
}

#[cfg(test)]
mod pubsub_routing_tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn one_publish_reaches_only_the_matching_stream_in_order() {
        let ps = PubSub::new();

        // The exact pickers used in subscriptions.rs.
        let mut cm = stream_for(&ps, "0xme", |e| match e {
            SocialEvent::CommunityMember(u) => Some(u.clone()),
            _ => None,
        });
        let mut fr = stream_for(&ps, "0xme", |e| match e {
            SocialEvent::Friendship(u) => Some(u.clone()),
            _ => None,
        });
        let mut blk = stream_for(&ps, "0xme", |e| match e {
            SocialEvent::Block(u) => Some(u.clone()),
            _ => None,
        });
        let mut pv = stream_for(&ps, "0xme", |e| match e {
            SocialEvent::PrivateVoice(u) => Some(u.clone()),
            _ => None,
        });
        let mut cv = stream_for(&ps, "0xme", |e| match e {
            SocialEvent::CommunityVoice(u) => Some(u.clone()),
            _ => None,
        });
        // One raw subscribe for the FriendConnectivity path (the manual loop's shape).
        let mut fc_rx = ps.subscribe("0xme");

        ps.publish(
            "0xme",
            SocialEvent::CommunityMember(CommunityMemberConnectivityUpdate {
                community_id: "a".into(),
                member: None,
                status: 0,
            }),
        );
        ps.publish(
            "0xme",
            SocialEvent::CommunityMember(CommunityMemberConnectivityUpdate {
                community_id: "b".into(),
                member: None,
                status: 0,
            }),
        );
        ps.publish("0xme", SocialEvent::Friendship(FriendshipUpdate::default()));

        // Community-member stream: both events, in order.
        assert_eq!(cm.next().await.unwrap().community_id, "a");
        assert_eq!(cm.next().await.unwrap().community_id, "b");

        // Friendship stream: exactly the friendship update.
        assert_eq!(fr.next().await.unwrap(), FriendshipUpdate::default());

        // Raw receiver sees all three Arcs, in order, undiscriminated.
        assert!(
            matches!(&*fc_rx.recv().await.unwrap(), SocialEvent::CommunityMember(u) if u.community_id == "a")
        );
        assert!(
            matches!(&*fc_rx.recv().await.unwrap(), SocialEvent::CommunityMember(u) if u.community_id == "b")
        );
        assert!(matches!(
            &*fc_rx.recv().await.unwrap(),
            SocialEvent::Friendship(_)
        ));

        // The non-matching streams yield nothing.
        assert!(timeout(Duration::from_millis(50), blk.next())
            .await
            .is_err());
        assert!(timeout(Duration::from_millis(50), pv.next()).await.is_err());
        assert!(timeout(Duration::from_millis(50), cv.next()).await.is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(limit: i32, offset: i32) -> Option<Pagination> {
        Some(Pagination { limit, offset })
    }

    #[test]
    fn friends_and_mutual_allow_up_to_1000_and_do_not_share_the_100_cap() {
        // The Unity/godot prewarm asks for 1000 in one shot; it must reach the query uncapped.
        assert_eq!(page_friends(&pg(1000, 0)), (1000, 0));
        // A larger ask is clamped to the friends max, still far above the old 100.
        assert_eq!(page_friends(&pg(5000, 0)), (1000, 0));
        // An unset/zero page size defaults to 1000, not 20.
        assert_eq!(page_friends(&pg(0, 0)), (1000, 0));
        assert_eq!(page_friends(&None), (1000, 0));
        // Regression guard: friends/mutual must never be capped at 100.
        assert_ne!(page_friends(&pg(1000, 0)).0, 100);
    }

    #[test]
    fn friendship_requests_default_100_max_200() {
        assert_eq!(page_friendship_requests(&pg(0, 0)), (100, 0));
        assert_eq!(page_friendship_requests(&None), (100, 0));
        assert_eq!(page_friendship_requests(&pg(150, 0)), (150, 0));
        assert_eq!(page_friendship_requests(&pg(1000, 0)), (200, 0));
    }

    #[test]
    fn blocked_users_default_and_max_200() {
        assert_eq!(page_blocked_users(&pg(0, 0)), (200, 0));
        assert_eq!(page_blocked_users(&None), (200, 0));
        assert_eq!(page_blocked_users(&pg(1000, 0)), (200, 0));
    }

    #[test]
    fn negative_page_size_falls_back_to_default_and_offset_is_bounded() {
        assert_eq!(page_friends(&pg(-1, -5)), (1000, 0));
        assert_eq!(
            page_friends(&pg(10, 10_000_000)),
            (10, MAX_PAGINATION_OFFSET)
        );
    }

    #[test]
    fn page_of_matches_upstream_get_page() {
        assert_eq!(page_of(1000, 0), 1);
        assert_eq!(page_of(100, 100), 2);
        assert_eq!(page_of(100, 250), 4);
        assert_eq!(page_of(0, 50), 1);
    }
}
