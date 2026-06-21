use super::super::domain::{fan_community_voice, require_moderator};
use super::super::helpers::SocialError;
use super::SocialServiceImpl;
use crate::rpc::context::Context;
use crate::rpc::proto::errors::ForbiddenError;
use crate::rpc::proto::v2::*;
use dcl_rpc::service_module_definition::ProcedureContext;

impl SocialServiceImpl {
    pub(super) async fn start_community_voice_chat(
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

    pub(super) async fn join_community_voice_chat(
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

    pub(super) async fn request_to_speak_in_community_voice_chat(
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

    pub(super) async fn promote_speaker_in_community_voice_chat(
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

    pub(super) async fn demote_speaker_in_community_voice_chat(
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

    pub(super) async fn kick_player_from_community_voice_chat(
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

    pub(super) async fn reject_speak_request_in_community_voice_chat(
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

    pub(super) async fn end_community_voice_chat(
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

    pub(super) async fn mute_speaker_from_community_voice_chat(
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
