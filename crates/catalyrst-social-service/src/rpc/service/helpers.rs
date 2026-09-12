use crate::rpc::proto::errors::*;
use crate::rpc::proto::v2::*;
use crate::rpc::pubsub::{PubSub, SocialEvent};
use catalyrst_drpc::rpc_protocol::RemoteErrorResponse;
use catalyrst_drpc::stream_protocol::Generator;
use tokio::sync::broadcast::error::RecvError;

const FRIENDSHIP_REQUESTS_DEFAULT_LIMIT: i64 = 100;
const FRIENDSHIP_REQUESTS_MAX_LIMIT: i64 = 200;
const FRIENDS_DEFAULT_LIMIT: i64 = 1000;
const FRIENDS_MAX_LIMIT: i64 = 1000;
const BLOCKED_USERS_DEFAULT_LIMIT: i64 = 200;
const BLOCKED_USERS_MAX_LIMIT: i64 = 200;
const MAX_PAGINATION_OFFSET: i64 = 100_000;

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

/// Mirrors upstream `normalizePagination(bounds)`. A missing, zero or negative page size
/// falls back to `default_limit`; a larger one is clamped to `max_limit`; the offset is
/// clamped to [`MAX_PAGINATION_OFFSET`].
///
/// The caps are silent -- `PaginatedResponse` carries only `total` and `page` -- so each
/// maximum is set at or above the largest page a shipping client requests. A client raising
/// its page size past a cap needs the constant raised in the same change.
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

pub(super) fn page_friends(p: &Option<Pagination>) -> (i64, i64) {
    page_bounded(p, FRIENDS_DEFAULT_LIMIT, FRIENDS_MAX_LIMIT)
}

pub(super) fn page_friendship_requests(p: &Option<Pagination>) -> (i64, i64) {
    page_bounded(
        p,
        FRIENDSHIP_REQUESTS_DEFAULT_LIMIT,
        FRIENDSHIP_REQUESTS_MAX_LIMIT,
    )
}

pub(super) fn page_blocked_users(p: &Option<Pagination>) -> (i64, i64) {
    page_bounded(p, BLOCKED_USERS_DEFAULT_LIMIT, BLOCKED_USERS_MAX_LIMIT)
}

/// The 1-based page number for an **already-bounded** `(limit, offset)`, mirroring upstream
/// `getPage`. Callers must pass the same bounded values they handed to the query.
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

        assert_eq!(cm.next().await.unwrap().community_id, "a");
        assert_eq!(cm.next().await.unwrap().community_id, "b");

        assert_eq!(fr.next().await.unwrap(), FriendshipUpdate::default());

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
        assert_eq!(page_friends(&pg(1000, 0)), (1000, 0));
        assert_eq!(page_friends(&pg(5000, 0)), (1000, 0));
        assert_eq!(page_friends(&pg(0, 0)), (1000, 0));
        assert_eq!(page_friends(&None), (1000, 0));
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
