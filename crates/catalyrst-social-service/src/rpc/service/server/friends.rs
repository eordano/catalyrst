use super::super::domain::{
    friendship_action_status, friendship_requests, friendship_update_for, status_ok,
    user_action_valid, Action,
};
use super::super::helpers::{
    empty_friends_profiles, friendship_status_invalid, internal_err, invalid_req, is_eth_address,
    normalize, page, page_number, upsert_internal_error, SocialError,
};
use super::SocialServiceImpl;
use crate::rpc::context::Context;
use crate::rpc::proto::errors::InvalidFriendshipAction;
use crate::rpc::proto::v2::*;
use crate::rpc::pubsub::SocialEvent;
use dcl_rpc::service_module_definition::ProcedureContext;

impl SocialServiceImpl {
    pub(super) async fn get_friends(
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
            Ok::<_, crate::rpc::db::DbError>((friends, total))
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

    pub(super) async fn get_mutual_friends(
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
            Ok::<_, crate::rpc::db::DbError>((friends, total))
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

    pub(super) async fn get_pending_friendship_requests(
        &self,
        request: GetFriendshipRequestsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
        friendship_requests(&context, request, true).await
    }

    pub(super) async fn get_sent_friendship_requests(
        &self,
        request: GetFriendshipRequestsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
        friendship_requests(&context, request, false).await
    }

    pub(super) async fn get_friendship_status(
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
            let action = db
                .last_friendship_action(&me, &other)
                .await?
                .and_then(|last| friendship_action_status(&last, &me));
            let status = match action {
                Some(s) => s,
                None => {
                    if db.is_blocked(&me, &other).await? {
                        FriendshipStatus::Blocked
                    } else if db.is_blocked(&other, &me).await? {
                        FriendshipStatus::BlockedBy
                    } else {
                        FriendshipStatus::None
                    }
                }
            };
            Ok::<_, crate::rpc::db::DbError>(status)
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

    pub(super) async fn upsert_friendship(
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
}
