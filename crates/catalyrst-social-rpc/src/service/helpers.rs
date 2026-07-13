use crate::proto::errors::*;
use crate::proto::v2::*;
use crate::pubsub::{PubSub, SocialEvent};
use dcl_rpc::rpc_protocol::RemoteErrorResponse;
use dcl_rpc::stream_protocol::Generator;
use tokio::sync::broadcast::error::RecvError;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

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

impl From<crate::db::DbError> for SocialError {
    fn from(e: crate::db::DbError) -> Self {
        SocialError::Internal(e.to_string())
    }
}

pub(super) fn normalize(addr: &str) -> String {
    addr.trim().to_lowercase()
}

pub(super) fn page(p: &Option<Pagination>) -> (i64, i64) {
    match p {
        Some(p) => {
            let mut limit = if p.limit <= 0 {
                DEFAULT_LIMIT
            } else {
                p.limit as i64
            };
            if limit > MAX_LIMIT {
                limit = MAX_LIMIT;
            }
            let offset = if p.offset < 0 { 0 } else { p.offset as i64 };
            (limit, offset)
        }
        None => (DEFAULT_LIMIT, 0),
    }
}

fn get_page(limit: i64, offset: i64) -> i32 {
    if limit <= 0 {
        return 1;
    }
    let off = offset.max(0);
    (((off as f64) / (limit as f64)).ceil() as i64 + 1) as i32
}

pub(super) fn page_number(p: &Option<Pagination>) -> i32 {
    match p {
        Some(p) => {
            let limit = if p.limit <= 0 {
                DEFAULT_LIMIT
            } else {
                p.limit as i64
            };
            let offset = if p.offset < 0 { 0 } else { p.offset as i64 };
            get_page(limit, offset)
        }
        None => 1,
    }
}

pub(super) fn requests_page_number(p: &Option<Pagination>, total: i64) -> i32 {
    match p {
        Some(p) => {
            let limit = if p.limit <= 0 { total } else { p.limit as i64 };
            let offset = if p.offset < 0 { 0 } else { p.offset as i64 };
            get_page(limit, offset)
        }
        None => get_page(total, 0),
    }
}

pub(super) fn is_eth_address(addr: &str) -> bool {
    match addr.strip_prefix("0x") {
        Some(s) => s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
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
    F: Fn(SocialEvent) -> Option<T> + Send + 'static,
{
    let (generator, yielder) = Generator::create();
    let mut rx = pubsub.subscribe(address);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(item) = pick(event) {
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
