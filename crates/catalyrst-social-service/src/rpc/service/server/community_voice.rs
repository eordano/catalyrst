use super::super::domain::{
    fan_community_voice, require_moderator, require_moderator_protecting_owner,
    validate_community_voice_participation, validate_community_voice_target_membership,
};
use super::super::helpers::normalize;
use super::super::helpers::SocialError;
use super::SocialServiceImpl;
use crate::rpc::context::Context;
use crate::rpc::proto::errors::ForbiddenError;
use crate::rpc::proto::v2::*;
use catalyrst_drpc::service_module_definition::ProcedureContext;

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

        // Re-read the ban now that the seat exists (upstream #482). The role read above is one
        // network round trip earlier; a ban committing in between runs its eviction while there is
        // still nobody to evict, so it no-ops and this join would hand a live seat to someone
        // already banned. Ordering the two checks around the seat makes them cover each other: a
        // ban landing before this read is caught here, and one landing after finds the participant
        // it needs to remove. An eviction that fails is logged and still refuses.
        if db.is_member_banned(&request.community_id, &me).await? {
            tracing::warn!(
                community_id = %request.community_id,
                address = %me,
                "banned while joining community voice chat; evicting"
            );
            if let Err(e) = context
                .server_context
                .gatekeeper()
                .kick_player(&request.community_id, &me)
                .await
            {
                tracing::error!(
                    community_id = %request.community_id,
                    address = %me,
                    error = %e,
                    "failed to evict after a racing ban"
                );
            }
            return Ok(JoinCommunityVoiceChatResponse {
                response: Some(
                    join_community_voice_chat_response::Response::ForbiddenError(ForbiddenError {
                        message: Some("banned from this community".into()),
                    }),
                ),
            });
        }

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
        // Entitlement gates gaining a capability, never giving one up: only raising a hand runs the
        // participation gate, so lowering a hand keeps working for someone since banned or gone.
        // Privacy-aware: a public community admits a guest holding no role (upstream now permits
        // this); a private community still requires membership; a banned actor is refused.
        if request.is_raising_hand {
            if let Err(f) =
                validate_community_voice_participation(db, &request.community_id, &me).await?
            {
                return Ok(RequestToSpeakInCommunityVoiceChatResponse {
                    response: Some(
                        request_to_speak_in_community_voice_chat_response::Response::ForbiddenError(
                            f,
                        ),
                    ),
                });
            }
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
        if let Err(f) = require_moderator_protecting_owner(
            db,
            &request.community_id,
            &me,
            &request.user_address,
            "promote speakers",
        )
        .await?
        {
            return Ok(PromoteSpeakerInCommunityVoiceChatResponse {
                response: Some(
                    promote_speaker_in_community_voice_chat_response::Response::ForbiddenError(f),
                ),
            });
        }
        if let Err(f) = validate_community_voice_target_membership(
            db,
            &request.community_id,
            &request.user_address,
        )
        .await?
        {
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
        if let Err(f) = require_moderator_protecting_owner(
            db,
            &request.community_id,
            &me,
            &request.user_address,
            "demote other speakers",
        )
        .await?
        {
            return Ok(DemoteSpeakerInCommunityVoiceChatResponse {
                response: Some(
                    demote_speaker_in_community_voice_chat_response::Response::ForbiddenError(f),
                ),
            });
        }
        if let Err(f) = validate_community_voice_target_membership(
            db,
            &request.community_id,
            &request.user_address,
        )
        .await?
        {
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
        if let Err(f) = require_moderator_protecting_owner(
            db,
            &request.community_id,
            &me,
            &request.user_address,
            "kick players",
        )
        .await?
        {
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
        if let Err(f) = require_moderator_protecting_owner(
            db,
            &request.community_id,
            &me,
            &request.user_address,
            "reject speak requests",
        )
        .await?
        {
            return Ok(RejectSpeakRequestInCommunityVoiceChatResponse {
                response: Some(
                    reject_speak_request_in_community_voice_chat_response::Response::ForbiddenError(
                        f,
                    ),
                ),
            });
        }
        if let Err(f) = validate_community_voice_target_membership(
            db,
            &request.community_id,
            &request.user_address,
        )
        .await?
        {
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
        // Acting on someone else needs the moderator gate. A self-action does not -- but it is not
        // an unconditional bypass either: only self-UNMUTE gains a capability, so it runs the same
        // privacy-aware participation gate as request-to-speak (self-mute always works, so that a
        // member since banned or gone can still silence themselves), mirroring upstream #447.
        let is_self_action = normalize(&request.user_address) == me;
        if is_self_action {
            if !request.muted {
                if let Err(f) =
                    validate_community_voice_participation(db, &request.community_id, &me).await?
                {
                    return Ok(MuteSpeakerFromCommunityVoiceChatResponse {
                        response: Some(
                            mute_speaker_from_community_voice_chat_response::Response::ForbiddenError(
                                f,
                            ),
                        ),
                    });
                }
            }
        } else if let Err(f) = require_moderator_protecting_owner(
            db,
            &request.community_id,
            &me,
            &request.user_address,
            "mute/unmute speakers",
        )
        .await?
        {
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
