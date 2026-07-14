use super::super::helpers::{
    internal_err, invalid_req, normalize, page_blocked_users, page_of, SocialError,
    FRIENDSHIP_RATE_LIMIT_MESSAGE,
};
use super::SocialServiceImpl;
use crate::rpc::context::{Context, PairScope};
use crate::rpc::proto::v2::*;
use crate::rpc::pubsub::SocialEvent;
use catalyrst_drpc::service_module_definition::ProcedureContext;

impl SocialServiceImpl {
    pub(super) async fn block_user(
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
        // Directional pair budget: the counterparty must never be able to spend the tokens the
        // blocker needs, or the account being blocked decides whether the block lands.
        if !context
            .server_context
            .friendship_limiter()
            .allow(&me, &other, PairScope::Directional)
        {
            tracing::warn!(actor = %me, target = %other, "block mutation rate limited");
            return Ok(BlockUserResponse {
                response: Some(block_user_response::Response::InvalidRequest(invalid_req(
                    FRIENDSHIP_RATE_LIMIT_MESSAGE,
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

    pub(super) async fn unblock_user(
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
        // Same directional bucket as block_user, so a block/unblock loop at one target shares one
        // budget.
        if !context
            .server_context
            .friendship_limiter()
            .allow(&me, &other, PairScope::Directional)
        {
            tracing::warn!(actor = %me, target = %other, "block mutation rate limited");
            return Ok(UnblockUserResponse {
                response: Some(unblock_user_response::Response::InvalidRequest(
                    invalid_req(FRIENDSHIP_RATE_LIMIT_MESSAGE),
                )),
            });
        }
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

    pub(super) async fn get_blocked_users(
        &self,
        request: GetBlockedUsersPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetBlockedUsersResponse, SocialError> {
        let me = Self::caller(&context)?;
        let db = context.server_context.db();
        // Bound the page with the blocked-users caps (default = max = 200); the blocklist is
        // otherwise fetched unbounded and every returned address then hits the profile lookup.
        let (limit, offset) = page_blocked_users(&request.pagination);
        let result = async {
            let rows = db.get_blocked_users(&me, limit, offset).await?;
            // total is the real row count, not the page length: clients page until they reach it.
            let total = db.count_blocked_users(&me).await?;
            Ok::<_, crate::rpc::db::DbError>((rows, total))
        }
        .await;
        let (rows, total) = match result {
            Ok(v) => v,
            Err(_) => {
                return Ok(GetBlockedUsersResponse {
                    profiles: Vec::new(),
                    pagination_data: Some(PaginatedResponse { total: 0, page: 1 }),
                })
            }
        };
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
                page: page_of(limit, offset),
            }),
        })
    }

    pub(super) async fn get_blocking_status(
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
}
