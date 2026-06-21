use super::super::helpers::{
    invalid_req, normalize, start_voice_conflict, start_voice_forbidden, start_voice_internal,
    start_voice_invalid, SocialError,
};
use super::SocialServiceImpl;
use crate::rpc::context::Context;
use crate::rpc::proto::errors::{ForbiddenError, NotFoundError};
use crate::rpc::proto::v2::*;
use crate::rpc::pubsub::SocialEvent;
use dcl_rpc::service_module_definition::ProcedureContext;
use uuid::Uuid;

impl SocialServiceImpl {
    pub(super) async fn start_private_voice_chat(
        &self,
        request: StartPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<StartPrivateVoiceChatResponse, SocialError> {
        let me = Self::caller(&context)?;

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

        if callee == me {
            return Ok(start_voice_forbidden(
                "Cannot start a private voice chat with yourself",
            ));
        }
        if db.is_friendship_blocked(&me, &callee).await? {
            return Ok(start_voice_forbidden(
                "This action is not allowed because either you blocked this user or this user blocked you",
            ));
        }

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

    pub(super) async fn accept_private_voice_chat(
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

    pub(super) async fn reject_private_voice_chat(
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

    pub(super) async fn end_private_voice_chat(
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

    pub(super) async fn get_incoming_private_voice_chat_request(
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
}
