use crate::context::Context;
use crate::db::Db;
use crate::proto::errors::*;
use crate::proto::v2::*;
use crate::pubsub::{PubSub, SocialEvent};
use async_trait::async_trait;
use dcl_rpc::rpc_protocol::RemoteErrorResponse;
use dcl_rpc::service_module_definition::ProcedureContext;
use dcl_rpc::stream_protocol::Generator;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

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

fn normalize(addr: &str) -> String {
    addr.trim().to_lowercase()
}

fn page(p: &Option<Pagination>) -> (i64, i64) {
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

fn page_number(p: &Option<Pagination>) -> i32 {
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

fn requests_page_number(p: &Option<Pagination>, total: i64) -> i32 {
    match p {
        Some(p) => {
            let limit = if p.limit <= 0 { total } else { p.limit as i64 };
            let offset = if p.offset < 0 { 0 } else { p.offset as i64 };
            get_page(limit, offset)
        }
        None => get_page(total, 0),
    }
}

fn is_eth_address(addr: &str) -> bool {
    match addr.strip_prefix("0x") {
        Some(s) => s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

fn empty_friends_profiles() -> PaginatedFriendsProfilesResponse {
    PaginatedFriendsProfilesResponse {
        friends: Vec::new(),
        pagination_data: Some(PaginatedResponse { total: 0, page: 1 }),
    }
}

fn friendship_status_invalid(msg: impl Into<String>) -> GetFriendshipStatusResponse {
    GetFriendshipStatusResponse {
        response: Some(get_friendship_status_response::Response::InvalidRequest(
            invalid_req(msg),
        )),
    }
}

fn upsert_internal_error(msg: impl Into<String>) -> UpsertFriendshipResponse {
    UpsertFriendshipResponse {
        response: Some(upsert_friendship_response::Response::InternalServerError(
            internal_err(msg),
        )),
    }
}

fn internal_err(msg: impl Into<String>) -> InternalServerError {
    // Callers pass raw error text (e.to_string()); log it server-side but return
    // a generic message so DB/schema internals aren't disclosed to RPC clients.
    let detail = msg.into();
    tracing::error!(detail = %detail, "social-rpc internal error");
    InternalServerError {
        message: Some("internal error".to_string()),
    }
}

fn invalid_req(msg: impl Into<String>) -> InvalidRequest {
    InvalidRequest {
        message: Some(msg.into()),
    }
}

fn start_voice_invalid(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(start_private_voice_chat_response::Response::InvalidRequest(
            invalid_req(msg),
        )),
    }
}

fn start_voice_conflict(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(
            start_private_voice_chat_response::Response::ConflictingError(ConflictingError {
                message: Some(msg.into()),
            }),
        ),
    }
}

fn start_voice_forbidden(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(start_private_voice_chat_response::Response::ForbiddenError(
            ForbiddenError {
                message: Some(msg.into()),
            },
        )),
    }
}

fn start_voice_internal(msg: impl Into<String>) -> StartPrivateVoiceChatResponse {
    StartPrivateVoiceChatResponse {
        response: Some(
            start_private_voice_chat_response::Response::InternalServerError(internal_err(msg)),
        ),
    }
}

fn stream_for<T, F>(pubsub: &PubSub, address: &str, pick: F) -> Generator<T>
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
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    });
    generator
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Request,
    Accept,
    Cancel,
    Reject,
    Delete,
    Block,
}

impl Action {
    fn as_str(self) -> &'static str {
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

    fn implies_active(self) -> bool {
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

fn user_action_valid(
    acting_user: &str,
    new_action: Action,
    new_user: &str,
    last: Option<&crate::db::LastAction>,
) -> bool {
    match last {
        None => {
            if new_action == Action::Request && acting_user == new_user {
                return false;
            }
            true
        }
        Some(last) => {
            let last_act = Action::from_str(&last.action);
            if !transition_valid(last_act, new_action) {
                return false;
            }
            if last.acting_user == acting_user {
                !matches!(new_action, Action::Accept | Action::Reject)
            } else {
                new_action != Action::Cancel
            }
        }
    }
}

pub struct SocialServiceImpl;

impl SocialServiceImpl {
    fn caller(ctx: &ProcedureContext<Context>) -> Result<String, SocialError> {
        ctx.server_context
            .identity(ctx.transport_id)
            .map(|a| normalize(&a))
            .ok_or(SocialError::Unauthenticated)
    }
}

#[async_trait]
impl SocialServiceServer<Context, SocialError> for SocialServiceImpl {
    async fn get_friends(
        &self,
        request: GetFriendsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendsProfilesResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        let (limit, offset) = page(&request.pagination);
        let result = async {
            let friends = db.get_friends(&me, limit, offset).await?;
            let total = db.count_friends(&me).await?;
            Ok::<_, crate::db::DbError>((friends, total))
        }
        .await;
        let (friends, total) = match result {
            Ok(v) => v,
            Err(_) => return Ok(empty_friends_profiles()),
        };
        let profiles = context
            .server_context
            .profiles()
            .friend_profiles(&friends)
            .await;
        Ok(PaginatedFriendsProfilesResponse {
            friends: profiles,
            pagination_data: Some(PaginatedResponse {
                total: total as i32,
                page: page_number(&request.pagination),
            }),
        })
    }

    async fn get_mutual_friends(
        &self,
        request: GetMutualFriendsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendsProfilesResponse, SocialError> {
        let me = Self::caller(&context)?;
        let other = match request.user.as_ref().map(|u| normalize(&u.address)) {
            Some(a) if is_eth_address(&a) => a,
            _ => return Ok(empty_friends_profiles()),
        };
        let db = context.server_context.db();
        let (limit, offset) = page(&request.pagination);
        let result = async {
            let friends = db.get_mutual_friends(&me, &other, limit, offset).await?;
            let total = db.count_mutual_friends(&me, &other).await?;
            Ok::<_, crate::db::DbError>((friends, total))
        }
        .await;
        let (friends, total) = match result {
            Ok(v) => v,
            Err(_) => return Ok(empty_friends_profiles()),
        };
        let profiles = context
            .server_context
            .profiles()
            .friend_profiles(&friends)
            .await;
        Ok(PaginatedFriendsProfilesResponse {
            friends: profiles,
            pagination_data: Some(PaginatedResponse {
                total: total as i32,
                page: page_number(&request.pagination),
            }),
        })
    }

    async fn get_pending_friendship_requests(
        &self,
        request: GetFriendshipRequestsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
        friendship_requests(&context, request, true).await
    }

    async fn get_sent_friendship_requests(
        &self,
        request: GetFriendshipRequestsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
        friendship_requests(&context, request, false).await
    }

    async fn get_friendship_status(
        &self,
        request: GetFriendshipStatusPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetFriendshipStatusResponse, SocialError> {
        let me = Self::caller(&context)?;
        let raw = request.user.as_ref().map(|u| u.address.clone());
        let other = match raw {
            Some(a) if !a.trim().is_empty() => {
                let n = normalize(&a);
                if !is_eth_address(&n) {
                    return Ok(friendship_status_invalid(
                        "Invalid user address in the request payload",
                    ));
                }
                n
            }
            _ => {
                return Ok(friendship_status_invalid(
                    "User address is missing in the request payload",
                ))
            }
        };
        let db = context.server_context.db();

        let resolved = async {
            let status = match db.last_friendship_action(&me, &other).await? {
                None => {
                    if db.is_blocked(&me, &other).await? {
                        FriendshipStatus::Blocked
                    } else if db.is_blocked(&other, &me).await? {
                        FriendshipStatus::BlockedBy
                    } else {
                        FriendshipStatus::None
                    }
                }
                Some(last) => {
                    let acting_is_me = last.acting_user == me;
                    match Action::from_str(&last.action) {
                        Some(Action::Accept) => FriendshipStatus::Accepted,
                        Some(Action::Cancel) => FriendshipStatus::Canceled,
                        Some(Action::Delete) => FriendshipStatus::Deleted,
                        Some(Action::Reject) => FriendshipStatus::Rejected,
                        Some(Action::Request) if acting_is_me => FriendshipStatus::RequestSent,
                        Some(Action::Request) => FriendshipStatus::RequestReceived,
                        Some(Action::Block) if acting_is_me => FriendshipStatus::Blocked,
                        Some(Action::Block) => FriendshipStatus::BlockedBy,
                        None => FriendshipStatus::None,
                    }
                }
            };
            Ok::<_, crate::db::DbError>(status)
        }
        .await;
        match resolved {
            Ok(status) => Ok(status_ok(status)),
            Err(e) => Ok(GetFriendshipStatusResponse {
                response: Some(
                    get_friendship_status_response::Response::InternalServerError(internal_err(
                        e.to_string(),
                    )),
                ),
            }),
        }
    }

    async fn upsert_friendship(
        &self,
        request: UpsertFriendshipPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UpsertFriendshipResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();

        let (action, other, message) = match request.action {
            Some(upsert_friendship_payload::Action::Request(p)) => (
                Action::Request,
                p.user.map(|u| normalize(&u.address)).unwrap_or_default(),
                p.message,
            ),
            Some(upsert_friendship_payload::Action::Accept(p)) => (
                Action::Accept,
                p.user.map(|u| normalize(&u.address)).unwrap_or_default(),
                None,
            ),
            Some(upsert_friendship_payload::Action::Reject(p)) => (
                Action::Reject,
                p.user.map(|u| normalize(&u.address)).unwrap_or_default(),
                None,
            ),
            Some(upsert_friendship_payload::Action::Delete(p)) => (
                Action::Delete,
                p.user.map(|u| normalize(&u.address)).unwrap_or_default(),
                None,
            ),
            Some(upsert_friendship_payload::Action::Cancel(p)) => (
                Action::Cancel,
                p.user.map(|u| normalize(&u.address)).unwrap_or_default(),
                None,
            ),
            None => {
                return Ok(UpsertFriendshipResponse {
                    response: Some(upsert_friendship_response::Response::InvalidRequest(
                        invalid_req("missing action"),
                    )),
                })
            }
        };

        if other.is_empty() || other == me {
            return Ok(UpsertFriendshipResponse {
                response: Some(upsert_friendship_response::Response::InvalidRequest(
                    invalid_req("invalid target user"),
                )),
            });
        }

        let blocked = match db.is_friendship_blocked(&me, &other).await {
            Ok(v) => v,
            Err(e) => return Ok(upsert_internal_error(e.to_string())),
        };
        if blocked {
            return Ok(UpsertFriendshipResponse {
                response: Some(
                    upsert_friendship_response::Response::InvalidFriendshipAction(
                        InvalidFriendshipAction {
                            message: Some(
                                "This action is not allowed because either you blocked this user or this user blocked you"
                                    .into(),
                            ),
                        },
                    ),
                ),
            });
        }

        let last = match db.last_friendship_action(&me, &other).await {
            Ok(v) => v,
            Err(e) => return Ok(upsert_internal_error(e.to_string())),
        };
        if !user_action_valid(&me, action, &other, last.as_ref()) {
            return Ok(UpsertFriendshipResponse {
                response: Some(
                    upsert_friendship_response::Response::InvalidFriendshipAction(
                        InvalidFriendshipAction {
                            message: Some(format!("invalid transition to {}", action.as_str())),
                        },
                    ),
                ),
            });
        }

        let existing = last.as_ref().map(|l| l.friendship_id);
        let (id, created_at) = match db
            .apply_friendship_action(
                &me,
                &other,
                action.as_str(),
                action.implies_active(),
                existing,
                message.as_deref(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Ok(UpsertFriendshipResponse {
                    response: Some(upsert_friendship_response::Response::InternalServerError(
                        internal_err(e.to_string()),
                    )),
                })
            }
        };
        let created_ms = created_at.timestamp_millis();

        let profiles = context.server_context.profiles();
        let my_profile = profiles.friend_profile(&me).await;
        let update =
            friendship_update_for(action, &me, &id, created_ms, message.as_deref(), my_profile);
        context
            .server_context
            .pubsub()
            .publish(&other, SocialEvent::Friendship(update));

        let other_profile = profiles.friend_profile(&other).await;
        Ok(UpsertFriendshipResponse {
            response: Some(upsert_friendship_response::Response::Accepted(
                upsert_friendship_response::Accepted {
                    id: id.to_string(),
                    created_at: created_ms,
                    friend: Some(other_profile),
                    message,
                },
            )),
        })
    }

    async fn block_user(
        &self,
        request: BlockUserPayload,
        context: ProcedureContext<Context>,
    ) -> Result<BlockUserResponse, SocialError> {
        let me = Self::caller(&context)?;
        let other = match request.user.as_ref() {
            Some(u) => normalize(&u.address),
            None => {
                return Ok(BlockUserResponse {
                    response: Some(block_user_response::Response::InvalidRequest(invalid_req(
                        "missing user",
                    ))),
                })
            }
        };
        if other == me {
            return Ok(BlockUserResponse {
                response: Some(block_user_response::Response::InvalidRequest(invalid_req(
                    "cannot block yourself",
                ))),
            });
        }
        let db = context.server_context.db();
        if let Err(e) = db.block_user(&me, &other).await {
            return Ok(BlockUserResponse {
                response: Some(block_user_response::Response::InternalServerError(
                    internal_err(e.to_string()),
                )),
            });
        }

        match db.last_friendship_action(&me, &other).await {
            Ok(Some(last)) => {
                let _ = db
                    .apply_friendship_action(
                        &me,
                        &other,
                        "block",
                        false,
                        Some(last.friendship_id),
                        None,
                    )
                    .await;
            }
            Ok(None) => {}
            Err(e) => {
                return Ok(BlockUserResponse {
                    response: Some(block_user_response::Response::InternalServerError(
                        internal_err(e.to_string()),
                    )),
                })
            }
        }

        let pubsub = context.server_context.pubsub();

        pubsub.publish(
            &other,
            SocialEvent::Block(BlockUpdate {
                address: me.clone(),
                is_blocked: true,
            }),
        );
        pubsub.publish(
            &other,
            SocialEvent::Friendship(FriendshipUpdate {
                update: Some(friendship_update::Update::Block(
                    friendship_update::BlockResponse {
                        user: Some(User {
                            address: me.clone(),
                        }),
                    },
                )),
            }),
        );

        let profile = context
            .server_context
            .profiles()
            .blocked_profile(&other, Some(chrono::Utc::now().timestamp_millis()))
            .await;
        Ok(BlockUserResponse {
            response: Some(block_user_response::Response::Ok(block_user_response::Ok {
                profile: Some(profile),
            })),
        })
    }

    async fn unblock_user(
        &self,
        request: UnblockUserPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UnblockUserResponse, SocialError> {
        let me = Self::caller(&context)?;
        let other = match request.user.as_ref() {
            Some(u) => normalize(&u.address),
            None => {
                return Ok(UnblockUserResponse {
                    response: Some(unblock_user_response::Response::InvalidRequest(
                        invalid_req("missing user"),
                    )),
                })
            }
        };
        let db = context.server_context.db();
        if let Err(e) = db.unblock_user(&me, &other).await {
            return Ok(UnblockUserResponse {
                response: Some(unblock_user_response::Response::InternalServerError(
                    internal_err(e.to_string()),
                )),
            });
        }
        let pubsub = context.server_context.pubsub();

        let last = match db.last_friendship_action(&me, &other).await {
            Ok(v) => v,
            Err(e) => {
                return Ok(UnblockUserResponse {
                    response: Some(unblock_user_response::Response::InternalServerError(
                        internal_err(e.to_string()),
                    )),
                })
            }
        };
        if let Some(last) = last {
            if db
                .apply_friendship_action(
                    &me,
                    &other,
                    "delete",
                    false,
                    Some(last.friendship_id),
                    None,
                )
                .await
                .is_ok()
            {
                pubsub.publish(
                    &other,
                    SocialEvent::Friendship(FriendshipUpdate {
                        update: Some(friendship_update::Update::Delete(
                            friendship_update::DeleteResponse {
                                user: Some(User {
                                    address: me.clone(),
                                }),
                            },
                        )),
                    }),
                );
            }
        }

        pubsub.publish(
            &other,
            SocialEvent::Block(BlockUpdate {
                address: me.clone(),
                is_blocked: false,
            }),
        );
        let profile = context
            .server_context
            .profiles()
            .blocked_profile(&other, None)
            .await;
        Ok(UnblockUserResponse {
            response: Some(unblock_user_response::Response::Ok(
                unblock_user_response::Ok {
                    profile: Some(profile),
                },
            )),
        })
    }

    async fn get_blocked_users(
        &self,
        request: GetBlockedUsersPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetBlockedUsersResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        let rows = match db.get_blocked_users(&me, i64::MAX, 0).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(GetBlockedUsersResponse {
                    profiles: Vec::new(),
                    pagination_data: Some(PaginatedResponse { total: 0, page: 1 }),
                })
            }
        };
        let total = rows.len() as i64;
        let addrs: Vec<String> = rows.iter().map(|r| r.address.clone()).collect();
        let map = context.server_context.profiles().get_profiles(&addrs).await;
        let profiles = rows
            .iter()
            .map(|r| {
                let blocked_at = Some(r.blocked_at.timestamp_millis());
                match map.get(&r.address.to_lowercase()) {
                    Some(info) => BlockedUserProfile {
                        address: r.address.clone(),
                        name: info.name.clone(),
                        has_claimed_name: info.has_claimed_name,
                        profile_picture_url: info.profile_picture_url.clone(),
                        blocked_at,
                        name_color: info.name_color.clone(),
                    },
                    None => BlockedUserProfile {
                        address: r.address.clone(),
                        name: String::new(),
                        has_claimed_name: false,
                        profile_picture_url: String::new(),
                        blocked_at,
                        name_color: None,
                    },
                }
            })
            .collect();
        Ok(GetBlockedUsersResponse {
            profiles,
            pagination_data: Some(PaginatedResponse {
                total: total as i32,
                page: page_number(&request.pagination),
            }),
        })
    }

    async fn get_blocking_status(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetBlockingStatusResponse, SocialError> {
        let me = Self::caller(&context)?;
        let (blocked_users, blocked_by_users) = context
            .server_context
            .db()
            .get_blocking_status(&me)
            .await
            .unwrap_or_default();
        Ok(GetBlockingStatusResponse {
            blocked_users,
            blocked_by_users,
        })
    }

    async fn get_social_settings(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetSocialSettingsResponse, SocialError> {
        let me = Self::caller(&context)?;
        let row = match context.server_context.db().get_social_settings(&me).await {
            Ok(r) => r.unwrap_or_default(),
            Err(e) => {
                return Ok(GetSocialSettingsResponse {
                    response: Some(get_social_settings_response::Response::InternalServerError(
                        internal_err(e.to_string()),
                    )),
                })
            }
        };
        Ok(GetSocialSettingsResponse {
            response: Some(get_social_settings_response::Response::Ok(
                get_social_settings_response::Ok {
                    settings: Some(settings_to_proto(&row)),
                },
            )),
        })
    }

    async fn upsert_social_settings(
        &self,
        request: UpsertSocialSettingsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UpsertSocialSettingsResponse, SocialError> {
        let me = Self::caller(&context)?;
        let pmp = request
            .private_messages_privacy
            .map(|_| pmp_to_db(request.private_messages_privacy()));
        let bvis = request
            .blocked_users_messages_visibility
            .map(|_| bvis_to_db(request.blocked_users_messages_visibility()));
        let sreact = request
            .show_situation_reactions
            .map(|_| sreact_to_db(request.show_situation_reactions()));

        match context
            .server_context
            .db()
            .upsert_social_settings(&me, pmp.as_deref(), bvis.as_deref(), sreact.as_deref())
            .await
        {
            Ok(row) => Ok(UpsertSocialSettingsResponse {
                response: Some(upsert_social_settings_response::Response::Ok(
                    settings_to_proto(&row),
                )),
            }),
            Err(e) => Ok(UpsertSocialSettingsResponse {
                response: Some(
                    upsert_social_settings_response::Response::InternalServerError(internal_err(
                        e.to_string(),
                    )),
                ),
            }),
        }
    }

    async fn get_private_messages_settings(
        &self,
        request: GetPrivateMessagesSettingsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetPrivateMessagesSettingsResponse, SocialError> {
        let me = Self::caller(&context)?;
        let targets: Vec<String> = request.user.iter().map(|u| normalize(&u.address)).collect();

        const MAX_USER_ADDRESSES: usize = 50;
        if targets.len() > MAX_USER_ADDRESSES {
            return Ok(GetPrivateMessagesSettingsResponse {
                response: Some(
                    get_private_messages_settings_response::Response::InvalidRequest(invalid_req(
                        format!("Too many user addresses: {}", targets.len()),
                    )),
                ),
            });
        }
        let rows = match context
            .server_context
            .db()
            .private_messages_settings(&me, &targets)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(GetPrivateMessagesSettingsResponse {
                    response: Some(
                        get_private_messages_settings_response::Response::InternalServerError(
                            internal_err(e.to_string()),
                        ),
                    ),
                })
            }
        };
        let settings = rows
            .into_iter()
            .map(|(addr, privacy, is_friend)| {
                get_private_messages_settings_response::PrivateMessagesSettings {
                    user: Some(User { address: addr }),
                    private_messages_privacy: pmp_from_db(&privacy) as i32,
                    is_friend,
                }
            })
            .collect();
        Ok(GetPrivateMessagesSettingsResponse {
            response: Some(get_private_messages_settings_response::Response::Ok(
                get_private_messages_settings_response::Ok { settings },
            )),
        })
    }

    async fn subscribe_to_friendship_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<FriendshipUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::Friendship(u) => Some(u),
                _ => None,
            },
        ))
    }

    async fn subscribe_to_friend_connectivity_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<FriendConnectivityUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        let ctx = context.server_context.clone();

        let online_candidates: Vec<String> = {
            let friends = ctx.db().friend_addresses(&me).await.unwrap_or_default();
            friends.into_iter().filter(|f| ctx.is_online(f)).collect()
        };
        let online_now = ctx
            .db()
            .online_friends(&me, &online_candidates)
            .await
            .unwrap_or(online_candidates);
        let snapshot = ctx.profiles().friend_profiles(&online_now).await;

        let (generator, yielder) = Generator::create();
        let mut rx = ctx.pubsub().subscribe(&me);
        tokio::spawn(async move {
            for friend in snapshot {
                let update = FriendConnectivityUpdate {
                    friend: Some(friend),
                    status: ConnectivityStatus::Online as i32,
                };
                if yielder.r#yield(update).await.is_err() {
                    return;
                }
            }
            loop {
                match rx.recv().await {
                    Ok(SocialEvent::FriendConnectivity(u)) => {
                        if yielder.r#yield(u).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => continue,
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        Ok(generator)
    }

    async fn subscribe_to_block_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<BlockUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::Block(u) => Some(u),
                _ => None,
            },
        ))
    }

    async fn subscribe_to_community_member_connectivity_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<CommunityMemberConnectivityUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::CommunityMember(u) => Some(u),
                _ => None,
            },
        ))
    }

    async fn subscribe_to_private_voice_chat_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<PrivateVoiceChatUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::PrivateVoice(u) => Some(u),
                _ => None,
            },
        ))
    }

    async fn subscribe_to_community_voice_chat_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<CommunityVoiceChatUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::CommunityVoice(u) => Some(u),
                _ => None,
            },
        ))
    }

    async fn start_private_voice_chat(
        &self,
        request: StartPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<StartPrivateVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        // Upstream start-private-voice-chat.ts guards `!request.callee?.address`.
        let callee = match request.callee.as_ref() {
            Some(u) if !u.address.trim().is_empty() => normalize(&u.address),
            _ => {
                return Ok(start_voice_invalid(
                    "Callee address is missing in the request payload",
                ))
            }
        };

        let ctx = &context.server_context;
        let db = ctx.db();
        let gk = ctx.gatekeeper();

        // 1) Community voice-chat cross-busy guard (handler-level in upstream).
        //    A user already in a community voice chat cannot also be in a 1:1
        //    call. The caller branch is checked first, then the callee, each
        //    with its own distinct message. Errors fail closed (treated as a
        //    conflict) so we never let a second concurrent call through when the
        //    gatekeeper is unreachable.
        let is_caller_in_community = match gk.is_user_in_community_voice_chat(&me).await {
            Ok(v) => v,
            Err(e) => return Ok(start_voice_internal(format!("community voice status: {e}"))),
        };
        let is_callee_in_community = match gk.is_user_in_community_voice_chat(&callee).await {
            Ok(v) => v,
            Err(e) => return Ok(start_voice_internal(format!("community voice status: {e}"))),
        };
        if is_caller_in_community {
            return Ok(start_voice_conflict(
                "Cannot start private voice chat while in a community voice chat",
            ));
        }
        if is_callee_in_community {
            return Ok(start_voice_conflict(
                "Cannot start private voice chat: the callee is in a community voice chat",
            ));
        }

        // 2) Privacy/friendship eligibility gate (voice.ts startPrivateVoiceChat).
        //    If EITHER party's private-messages privacy is not `all` (default is
        //    `only_friends`), the two must be active friends, otherwise the call
        //    is forbidden.
        let callee_privacy = db
            .get_social_settings(&callee)
            .await?
            .map(|s| s.private_messages_privacy)
            .unwrap_or_else(|| "only_friends".into());
        let caller_privacy = db
            .get_social_settings(&me)
            .await?
            .map(|s| s.private_messages_privacy)
            .unwrap_or_else(|| "only_friends".into());
        if callee_privacy != "all" || caller_privacy != "all" {
            let is_active = db
                .friendship_is_active(&me, &callee)
                .await?
                .unwrap_or(false);
            if !is_active {
                return Ok(start_voice_forbidden(
                    "The callee or the caller are not accepting voice calls from users that are not friends",
                ));
            }
        }

        // 3) Symmetric DB busy guard (voiceDb.areUsersBeingCalledOrCallingSomeone):
        //    reject if either address already appears as a caller OR callee of an
        //    existing call — covers all four column/address combinations.
        if db
            .are_users_being_called_or_calling_someone(
                &[me.clone(), callee.clone()],
                ctx.cfg().private_voice_chat_expiration_ms,
            )
            .await?
        {
            return Ok(start_voice_conflict(
                "One of the users is busy calling someone else",
            ));
        }

        // 4) Live voice-chat gatekeeper guard for BOTH parties. Callee is checked
        //    first (mirroring upstream's `if (isCalleeInAVoiceChat) ... else if
        //    (isCallerInAVoiceChat)`), each surfacing the busy user's address.
        let is_caller_in_voice = match gk.is_user_in_a_voice_chat(&me).await {
            Ok(v) => v,
            Err(e) => return Ok(start_voice_internal(format!("voice status: {e}"))),
        };
        let is_callee_in_voice = match gk.is_user_in_a_voice_chat(&callee).await {
            Ok(v) => v,
            Err(e) => return Ok(start_voice_internal(format!("voice status: {e}"))),
        };
        if is_callee_in_voice {
            return Ok(start_voice_conflict(format!(
                "One of the users is already in a voice chat: {callee}"
            )));
        }
        if is_caller_in_voice {
            return Ok(start_voice_conflict(format!(
                "One of the users is already in a voice chat: {me}"
            )));
        }

        // 5) Record the call intent. Post-alignment (upstream migration
        //    `1749835946066`) the row carries no `expires_at`: `created_at`
        //    defaults to `now()` and the SAME config value
        //    (`PRIVATE_VOICE_CHAT_EXPIRATION_TIME`, ms) drives both the
        //    busy-filter window and the sweep window. The DB's unique
        //    caller/callee constraints are the final concurrency backstop; a
        //    violation surfaces as an internal error, matching upstream where the
        //    constraint throws.
        match db.start_private_voice_chat(&me, &callee).await {
            Ok(id) => {
                ctx.pubsub().publish(
                    &callee,
                    SocialEvent::PrivateVoice(PrivateVoiceChatUpdate {
                        call_id: id.to_string(),
                        status: PrivateVoiceChatStatus::VoiceChatRequested as i32,
                        caller: Some(User {
                            address: me.clone(),
                        }),
                        callee: Some(User {
                            address: callee.clone(),
                        }),
                        credentials: None,
                    }),
                );
                Ok(StartPrivateVoiceChatResponse {
                    response: Some(start_private_voice_chat_response::Response::Ok(
                        start_private_voice_chat_response::Ok {
                            call_id: id.to_string(),
                        },
                    )),
                })
            }
            Err(e) => Ok(start_voice_internal(e.to_string())),
        }
    }

    async fn accept_private_voice_chat(
        &self,
        request: AcceptPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<AcceptPrivateVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let id = match Uuid::parse_str(&request.call_id) {
            Ok(id) => id,
            Err(_) => {
                return Ok(AcceptPrivateVoiceChatResponse {
                    response: Some(
                        accept_private_voice_chat_response::Response::InvalidRequest(invalid_req(
                            "invalid call_id",
                        )),
                    ),
                })
            }
        };
        let ctx = &context.server_context;
        let chat = match ctx.db().get_private_voice_chat(id).await? {
            Some(c) => c,
            None => {
                return Ok(AcceptPrivateVoiceChatResponse {
                    response: Some(accept_private_voice_chat_response::Response::NotFound(
                        NotFoundError {
                            message: Some("call not found".into()),
                        },
                    )),
                })
            }
        };
        if chat.callee_address != me {
            return Ok(AcceptPrivateVoiceChatResponse {
                response: Some(
                    accept_private_voice_chat_response::Response::ForbiddenError(ForbiddenError {
                        message: Some("not the callee".into()),
                    }),
                ),
            });
        }
        // Mint both ends' LiveKit tokens in one gatekeeper call, then hand the
        // caller its URL via the pubsub event and the callee its URL in the
        // direct response (matches upstream voice.ts accept flow).
        let urls = ctx
            .gatekeeper()
            .private_voice_credentials(&request.call_id, &chat.callee_address, &chat.caller_address)
            .await;
        let caller_url = urls.get(&chat.caller_address.to_lowercase()).cloned();
        let callee_url = urls.get(&chat.callee_address.to_lowercase()).cloned();
        let creds = PrivateVoiceChatCredentials {
            connection_url: callee_url.unwrap_or_default(),
        };

        ctx.pubsub().publish(
            &chat.caller_address,
            SocialEvent::PrivateVoice(PrivateVoiceChatUpdate {
                call_id: request.call_id.clone(),
                status: PrivateVoiceChatStatus::VoiceChatAccepted as i32,
                caller: Some(User {
                    address: chat.caller_address.clone(),
                }),
                callee: Some(User {
                    address: chat.callee_address.clone(),
                }),
                credentials: Some(PrivateVoiceChatCredentials {
                    connection_url: caller_url.unwrap_or_default(),
                }),
            }),
        );

        Ok(AcceptPrivateVoiceChatResponse {
            response: Some(accept_private_voice_chat_response::Response::Ok(
                accept_private_voice_chat_response::Ok {
                    call_id: request.call_id,
                    credentials: Some(creds),
                },
            )),
        })
    }

    async fn reject_private_voice_chat(
        &self,
        request: RejectPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<RejectPrivateVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let id = match Uuid::parse_str(&request.call_id) {
            Ok(id) => id,
            Err(_) => {
                return Ok(RejectPrivateVoiceChatResponse {
                    response: Some(
                        reject_private_voice_chat_response::Response::InvalidRequest(invalid_req(
                            "invalid call_id",
                        )),
                    ),
                })
            }
        };
        let ctx = &context.server_context;
        let chat = match ctx.db().get_private_voice_chat(id).await? {
            Some(c) => c,
            None => {
                return Ok(RejectPrivateVoiceChatResponse {
                    response: Some(reject_private_voice_chat_response::Response::NotFound(
                        NotFoundError {
                            message: Some("call not found".into()),
                        },
                    )),
                })
            }
        };
        let _ = me;
        ctx.db().delete_private_voice_chat(id).await?;
        ctx.pubsub().publish(
            &chat.caller_address,
            SocialEvent::PrivateVoice(PrivateVoiceChatUpdate {
                call_id: request.call_id.clone(),
                status: PrivateVoiceChatStatus::VoiceChatRejected as i32,
                caller: Some(User {
                    address: chat.caller_address.clone(),
                }),
                callee: Some(User {
                    address: chat.callee_address.clone(),
                }),
                credentials: None,
            }),
        );
        Ok(RejectPrivateVoiceChatResponse {
            response: Some(reject_private_voice_chat_response::Response::Ok(
                reject_private_voice_chat_response::Ok {
                    call_id: request.call_id,
                },
            )),
        })
    }

    async fn end_private_voice_chat(
        &self,
        request: EndPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<EndPrivateVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let id = match Uuid::parse_str(&request.call_id) {
            Ok(id) => id,
            Err(_) => {
                return Ok(EndPrivateVoiceChatResponse {
                    response: Some(end_private_voice_chat_response::Response::NotFound(
                        NotFoundError {
                            message: Some("invalid call_id".into()),
                        },
                    )),
                })
            }
        };
        let ctx = &context.server_context;
        let chat = match ctx.db().get_private_voice_chat(id).await? {
            Some(c) => c,
            None => {
                return Ok(EndPrivateVoiceChatResponse {
                    response: Some(end_private_voice_chat_response::Response::NotFound(
                        NotFoundError {
                            message: Some("call not found".into()),
                        },
                    )),
                })
            }
        };
        ctx.db().delete_private_voice_chat(id).await?;
        // Tear down the LiveKit room so any in-progress media is dropped.
        ctx.gatekeeper()
            .end_private_voice_chat(&request.call_id, &me)
            .await;

        for target in [&chat.caller_address, &chat.callee_address] {
            ctx.pubsub().publish(
                target,
                SocialEvent::PrivateVoice(PrivateVoiceChatUpdate {
                    call_id: request.call_id.clone(),
                    status: PrivateVoiceChatStatus::VoiceChatEnded as i32,
                    caller: Some(User {
                        address: chat.caller_address.clone(),
                    }),
                    callee: Some(User {
                        address: chat.callee_address.clone(),
                    }),
                    credentials: None,
                }),
            );
        }
        Ok(EndPrivateVoiceChatResponse {
            response: Some(end_private_voice_chat_response::Response::Ok(
                end_private_voice_chat_response::Ok {
                    call_id: request.call_id,
                },
            )),
        })
    }

    async fn get_incoming_private_voice_chat_request(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetIncomingPrivateVoiceChatRequestResponse, SocialError> {
        let me = Self::caller(&context)?;
        match context
            .server_context
            .db()
            .incoming_private_voice_chat(&me)
            .await?
        {
            Some(chat) => Ok(GetIncomingPrivateVoiceChatRequestResponse {
                response: Some(
                    get_incoming_private_voice_chat_request_response::Response::Ok(
                        get_incoming_private_voice_chat_request_response::Ok {
                            caller: Some(User {
                                address: chat.caller_address,
                            }),
                            call_id: chat.id.to_string(),
                        },
                    ),
                ),
            }),
            None => Ok(GetIncomingPrivateVoiceChatRequestResponse {
                response: Some(
                    get_incoming_private_voice_chat_request_response::Response::NotFound(
                        NotFoundError {
                            message: Some("no incoming call".into()),
                        },
                    ),
                ),
            }),
        }
    }

    async fn start_community_voice_chat(
        &self,
        request: StartCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<StartCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        let role = match db.community_role(&request.community_id, &me).await? {
            Some(r) if r == "owner" || r == "moderator" => r,
            _ => {
                return Ok(StartCommunityVoiceChatResponse {
                    response: Some(
                        start_community_voice_chat_response::Response::ForbiddenError(
                            ForbiddenError {
                                message: Some("requires moderator or owner role".into()),
                            },
                        ),
                    ),
                })
            }
        };
        // action=create: opens the room and grants the moderator publish.
        let conn = context
            .server_context
            .gatekeeper()
            .community_voice_credentials(&request.community_id, &me, &role, "create", None)
            .await
            .unwrap_or_default();

        fan_community_voice(
            &context.server_context,
            &request.community_id,
            CommunityVoiceChatStatus::CommunityVoiceChatStarted,
            Some(me.as_str()),
        )
        .await;
        Ok(StartCommunityVoiceChatResponse {
            response: Some(start_community_voice_chat_response::Response::Ok(
                start_community_voice_chat_response::Ok {
                    credentials: Some(CommunityVoiceChatCredentials {
                        connection_url: conn,
                    }),
                },
            )),
        })
    }

    async fn join_community_voice_chat(
        &self,
        request: JoinCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<JoinCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        let role = match db.community_role(&request.community_id, &me).await? {
            Some(r) => r,
            None => {
                return Ok(JoinCommunityVoiceChatResponse {
                    response: Some(
                        join_community_voice_chat_response::Response::ForbiddenError(
                            ForbiddenError {
                                message: Some("not a community member".into()),
                            },
                        ),
                    ),
                })
            }
        };
        // action=join: listener by default; moderators/owners join as speakers.
        let conn = context
            .server_context
            .gatekeeper()
            .community_voice_credentials(&request.community_id, &me, &role, "join", None)
            .await
            .unwrap_or_default();
        Ok(JoinCommunityVoiceChatResponse {
            response: Some(join_community_voice_chat_response::Response::Ok(
                join_community_voice_chat_response::Ok {
                    voice_chat_id: request.community_id,
                    credentials: Some(CommunityVoiceChatCredentials {
                        connection_url: conn,
                    }),
                },
            )),
        })
    }

    async fn request_to_speak_in_community_voice_chat(
        &self,
        request: RequestToSpeakInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<RequestToSpeakInCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if db
            .community_role(&request.community_id, &me)
            .await?
            .is_none()
        {
            return Ok(RequestToSpeakInCommunityVoiceChatResponse {
                response: Some(
                    request_to_speak_in_community_voice_chat_response::Response::ForbiddenError(
                        ForbiddenError {
                            message: Some("not a community member".into()),
                        },
                    ),
                ),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .request_to_speak(&request.community_id, &me, request.is_raising_hand)
            .await;
        Ok(RequestToSpeakInCommunityVoiceChatResponse {
            response: Some(
                request_to_speak_in_community_voice_chat_response::Response::Ok(
                    request_to_speak_in_community_voice_chat_response::Ok {
                        message: "ok".into(),
                    },
                ),
            ),
        })
    }

    async fn promote_speaker_in_community_voice_chat(
        &self,
        request: PromoteSpeakerInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PromoteSpeakerInCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if let Err(f) = require_moderator(db, &request.community_id, &me).await? {
            return Ok(PromoteSpeakerInCommunityVoiceChatResponse {
                response: Some(
                    promote_speaker_in_community_voice_chat_response::Response::ForbiddenError(f),
                ),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .set_speaker(&request.community_id, &request.user_address, true)
            .await;
        Ok(PromoteSpeakerInCommunityVoiceChatResponse {
            response: Some(
                promote_speaker_in_community_voice_chat_response::Response::Ok(
                    promote_speaker_in_community_voice_chat_response::Ok {
                        message: "ok".into(),
                    },
                ),
            ),
        })
    }

    async fn demote_speaker_in_community_voice_chat(
        &self,
        request: DemoteSpeakerInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<DemoteSpeakerInCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if let Err(f) = require_moderator(db, &request.community_id, &me).await? {
            return Ok(DemoteSpeakerInCommunityVoiceChatResponse {
                response: Some(
                    demote_speaker_in_community_voice_chat_response::Response::ForbiddenError(f),
                ),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .set_speaker(&request.community_id, &request.user_address, false)
            .await;
        Ok(DemoteSpeakerInCommunityVoiceChatResponse {
            response: Some(
                demote_speaker_in_community_voice_chat_response::Response::Ok(
                    demote_speaker_in_community_voice_chat_response::Ok {
                        message: "ok".into(),
                    },
                ),
            ),
        })
    }

    async fn kick_player_from_community_voice_chat(
        &self,
        request: KickPlayerFromCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<KickPlayerFromCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if let Err(f) = require_moderator(db, &request.community_id, &me).await? {
            return Ok(KickPlayerFromCommunityVoiceChatResponse {
                response: Some(
                    kick_player_from_community_voice_chat_response::Response::ForbiddenError(f),
                ),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .kick_player(&request.community_id, &request.user_address)
            .await;
        Ok(KickPlayerFromCommunityVoiceChatResponse {
            response: Some(
                kick_player_from_community_voice_chat_response::Response::Ok(
                    kick_player_from_community_voice_chat_response::Ok {
                        message: "ok".into(),
                    },
                ),
            ),
        })
    }

    async fn reject_speak_request_in_community_voice_chat(
        &self,
        request: RejectSpeakRequestInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<RejectSpeakRequestInCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if let Err(f) = require_moderator(db, &request.community_id, &me).await? {
            return Ok(RejectSpeakRequestInCommunityVoiceChatResponse {
                response: Some(
                    reject_speak_request_in_community_voice_chat_response::Response::ForbiddenError(
                        f,
                    ),
                ),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .reject_speak_request(&request.community_id, &request.user_address)
            .await;
        Ok(RejectSpeakRequestInCommunityVoiceChatResponse {
            response: Some(
                reject_speak_request_in_community_voice_chat_response::Response::Ok(
                    reject_speak_request_in_community_voice_chat_response::Ok {
                        message: "ok".into(),
                    },
                ),
            ),
        })
    }

    async fn end_community_voice_chat(
        &self,
        request: EndCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<EndCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if let Err(f) = require_moderator(db, &request.community_id, &me).await? {
            return Ok(EndCommunityVoiceChatResponse {
                response: Some(end_community_voice_chat_response::Response::ForbiddenError(
                    f,
                )),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .end_community_voice_chat(&request.community_id, &me)
            .await;

        fan_community_voice(
            &context.server_context,
            &request.community_id,
            CommunityVoiceChatStatus::CommunityVoiceChatEnded,
            None,
        )
        .await;
        Ok(EndCommunityVoiceChatResponse {
            response: Some(end_community_voice_chat_response::Response::Ok(
                end_community_voice_chat_response::Ok {
                    message: "ok".into(),
                },
            )),
        })
    }

    async fn mute_speaker_from_community_voice_chat(
        &self,
        request: MuteSpeakerFromCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<MuteSpeakerFromCommunityVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        if let Err(f) = require_moderator(db, &request.community_id, &me).await? {
            return Ok(MuteSpeakerFromCommunityVoiceChatResponse {
                response: Some(
                    mute_speaker_from_community_voice_chat_response::Response::ForbiddenError(f),
                ),
            });
        }
        let _ = context
            .server_context
            .gatekeeper()
            .mute_speaker(&request.community_id, &request.user_address, request.muted)
            .await;
        Ok(MuteSpeakerFromCommunityVoiceChatResponse {
            response: Some(
                mute_speaker_from_community_voice_chat_response::Response::Ok(
                    mute_speaker_from_community_voice_chat_response::Ok {
                        muted: request.muted,
                    },
                ),
            ),
        })
    }
}

fn status_ok(status: FriendshipStatus) -> GetFriendshipStatusResponse {
    GetFriendshipStatusResponse {
        response: Some(get_friendship_status_response::Response::Accepted(
            get_friendship_status_response::Ok {
                status: status as i32,
                message: None,
            },
        )),
    }
}

fn settings_to_proto(row: &crate::db::SocialSettingsRow) -> SocialSettings {
    SocialSettings {
        private_messages_privacy: pmp_from_db(&row.private_messages_privacy) as i32,
        blocked_users_messages_visibility: bvis_from_db(&row.blocked_users_messages_visibility)
            as i32,
        show_situation_reactions: sreact_from_db(&row.show_situation_reactions) as i32,
    }
}

fn pmp_to_db(v: PrivateMessagePrivacySetting) -> String {
    match v {
        PrivateMessagePrivacySetting::All => "all",
        PrivateMessagePrivacySetting::OnlyFriends => "only_friends",
    }
    .to_string()
}
fn pmp_from_db(s: &str) -> PrivateMessagePrivacySetting {
    match s {
        "all" => PrivateMessagePrivacySetting::All,
        _ => PrivateMessagePrivacySetting::OnlyFriends,
    }
}

fn bvis_to_db(v: BlockedUsersMessagesVisibilitySetting) -> String {
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

fn sreact_to_db(v: SituationReactionsVisibility) -> String {
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

fn friendship_update_for(
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

async fn friendship_requests(
    context: &ProcedureContext<Context>,
    request: GetFriendshipRequestsPayload,
    incoming: bool,
) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
    let me = SocialServiceImpl::caller(context)?;
    let db = context.server_context.db();
    let (limit, offset) = page(&request.pagination);
    let fetched = async {
        let rows = db
            .get_friendship_requests(&me, incoming, limit, offset)
            .await?;
        let total = db.count_friendship_requests(&me, incoming).await?;
        Ok::<_, crate::db::DbError>((rows, total))
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
            page: requests_page_number(&request.pagination, total),
        }),
    })
}

async fn fan_community_voice(
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

async fn require_moderator(
    db: &Db,
    community_id: &str,
    address: &str,
) -> Result<Result<(), ForbiddenError>, SocialError> {
    match db.community_role(community_id, address).await? {
        Some(role) if role == "owner" || role == "moderator" => Ok(Ok(())),
        _ => Ok(Err(ForbiddenError {
            message: Some("requires moderator or owner role".into()),
        })),
    }
}
