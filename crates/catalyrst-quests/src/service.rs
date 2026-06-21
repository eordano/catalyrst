//! dcl-rpc `QuestsService` implementation — port of `decentraland/quests`
//! crates/server/src/rpc/service.rs over the in-process context. Implements the
//! six procedures from definitions.proto:
//!   StartQuest / AbortQuest / SendEvent / Subscribe / GetAllQuests /
//!   GetQuestDefinition
//! The per-connection user address comes from the signed auth-chain handshake
//! (see `ws.rs` / `auth_chain.rs`), keyed by transport id in `Context`.

use crate::context::Context;
use crate::proto::*;
use crate::quests::{self, QuestError};
use crate::state::{compute_instance_state_quest, hide_quest_actions, hide_state_actions};
use async_trait::async_trait;
use dcl_rpc::rpc_protocol::RemoteErrorResponse;
use dcl_rpc::service_module_definition::ProcedureContext;
use dcl_rpc::stream_protocol::Generator;
use tokio::sync::broadcast::error::RecvError;

pub struct QuestsServiceImpl;

impl QuestsServiceImpl {
    /// The authenticated user address for this transport, or the
    /// "transport id not registered" error (upstream `NotExistsTransportID`).
    fn user(ctx: &ProcedureContext<Context>) -> Result<String, ServiceError> {
        ctx.server_context
            .identity(ctx.transport_id)
            .ok_or(ServiceError::NotExistsTransportID)
    }
}

#[async_trait]
impl QuestsServiceServer<Context, ServiceError> for QuestsServiceImpl {
    async fn start_quest(
        &self,
        request: StartQuestRequest,
        context: ProcedureContext<Context>,
    ) -> Result<StartQuestResponse, ServiceError> {
        let user_address = Self::user(&context)?;
        let ctx = &context.server_context;
        let quest_id = request.quest_id;

        match quests::start_quest(ctx.db(), &user_address, &quest_id).await {
            Ok(new_instance_id) => {
                // Publish a NewQuestStarted update with the instance's initial,
                // action-hidden state (upstream StartQuest success path).
                match build_quest_instance(ctx, &quest_id, &new_instance_id).await {
                    Ok(instance) => {
                        ctx.pubsub().publish(UserUpdate {
                            message: Some(user_update::Message::NewQuestStarted(instance)),
                            user_address: user_address.clone(),
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "StartQuest > calculating initial state");
                    }
                }
                Ok(start_quest_accepted())
            }
            Err(QuestError::NotFoundOrInactive) => Ok(start_quest_invalid_quest()),
            Err(QuestError::QuestAlreadyStarted) => Ok(start_quest_already_started()),
            Err(QuestError::NotUuid) => Ok(start_quest_not_uuid()),
            Err(e) => {
                tracing::error!(error = %e, quest_id, "StartQuest error");
                Ok(start_quest_internal_error())
            }
        }
    }

    async fn abort_quest(
        &self,
        request: AbortQuestRequest,
        context: ProcedureContext<Context>,
    ) -> Result<AbortQuestResponse, ServiceError> {
        let user_address = Self::user(&context)?;
        let ctx = &context.server_context;
        match quests::abandon_quest(ctx.db(), &user_address, &request.quest_instance_id).await {
            Ok(_) => Ok(abort_quest_accepted()),
            Err(QuestError::NotInstanceOwner) => Ok(abort_quest_not_owner()),
            Err(QuestError::NotFound) => Ok(abort_quest_not_found()),
            Err(QuestError::NotUuid) => Ok(abort_quest_not_uuid()),
            Err(e) => {
                tracing::error!(error = %e, instance = %request.quest_instance_id, "AbortQuest error");
                Ok(abort_quest_internal_error())
            }
        }
    }

    async fn send_event(
        &self,
        request: EventRequest,
        context: ProcedureContext<Context>,
    ) -> Result<EventResponse, ServiceError> {
        let user_address = Self::user(&context)?;
        let ctx = &context.server_context;
        // Upstream's add_event_controller: NoAction -> Ignored; otherwise build
        // an Event with a fresh id and push it onto the processing queue.
        match quests::build_event(&user_address, request) {
            Some((id, event)) => {
                if ctx.push_event(event) {
                    Ok(event_accepted(&id.to_string()))
                } else {
                    Ok(event_internal_error())
                }
            }
            None => Ok(event_ignored()),
        }
    }

    async fn subscribe(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<UserUpdate>, ServiceError> {
        let user_address = Self::user(&context)?;
        let (generator, yielder) = Generator::create();

        // First message: subscribed=true (upstream Subscribe accepted_response).
        if yielder
            .r#yield(UserUpdate {
                message: Some(user_update::Message::Subscribed(true)),
                user_address: user_address.clone(),
            })
            .await
            .is_err()
        {
            return Err(ServiceError::InternalError);
        }

        let mut rx = context.server_context.pubsub().subscribe(&user_address);
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(update) => {
                        if yielder.r#yield(update).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });

        Ok(generator)
    }

    async fn get_all_quests(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetAllQuestsResponse, ServiceError> {
        let user_address = Self::user(&context)?;
        let ctx = &context.server_context;
        let instances = match ctx
            .db()
            .get_active_user_quest_instances(&user_address)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "GetAllQuests > load instances");
                return Ok(get_all_quests_internal_error());
            }
        };

        let mut quests = Vec::with_capacity(instances.len());
        for instance in instances {
            match build_quest_instance(ctx, &instance.quest_id, &instance.id).await {
                Ok(qi) => quests.push(qi),
                Err(e) => {
                    tracing::error!(error = %e, "GetAllQuests > compute state");
                    return Ok(get_all_quests_internal_error());
                }
            }
        }
        Ok(get_all_quests_ok(quests))
    }

    async fn get_quest_definition(
        &self,
        request: GetQuestDefinitionRequest,
        context: ProcedureContext<Context>,
    ) -> Result<GetQuestDefinitionResponse, ServiceError> {
        let ctx = &context.server_context;
        match ctx
            .db()
            .get_quest_with_decoded_definition(&request.quest_id)
            .await
        {
            Ok(mut quest) => {
                hide_quest_actions(&mut quest);
                Ok(get_quest_definition_ok(quest))
            }
            Err(e) => {
                tracing::error!(error = %e, quest_id = %request.quest_id, "GetQuestDefinition error");
                Ok(get_quest_definition_internal_error())
            }
        }
    }
}

/// Build a `QuestInstance` (id + full quest + action-hidden state) for a
/// running instance — used by StartQuest and GetAllQuests.
async fn build_quest_instance(
    ctx: &Context,
    quest_id: &str,
    instance_id: &str,
) -> Result<QuestInstance, QuestError> {
    let quest = ctx.db().get_quest_with_decoded_definition(quest_id).await?;
    let mut state = compute_instance_state_quest(ctx.db(), &quest, instance_id).await?;
    hide_state_actions(&mut state);
    Ok(QuestInstance {
        id: instance_id.to_string(),
        quest: Some(quest),
        state: Some(state),
    })
}

// ---- Response constructors (port of crates/protocol/src/quests/builders.rs) ----

fn start_quest_accepted() -> StartQuestResponse {
    StartQuestResponse {
        response: Some(start_quest_response::Response::Accepted(
            start_quest_response::Accepted {},
        )),
    }
}
fn start_quest_invalid_quest() -> StartQuestResponse {
    StartQuestResponse {
        response: Some(start_quest_response::Response::InvalidQuest(
            InvalidQuest {},
        )),
    }
}
fn start_quest_not_uuid() -> StartQuestResponse {
    StartQuestResponse {
        response: Some(start_quest_response::Response::NotUuidError(NotUuid {})),
    }
}
fn start_quest_already_started() -> StartQuestResponse {
    StartQuestResponse {
        response: Some(start_quest_response::Response::QuestAlreadyStarted(
            QuestAlreadyStarted {},
        )),
    }
}
fn start_quest_internal_error() -> StartQuestResponse {
    StartQuestResponse {
        response: Some(start_quest_response::Response::InternalServerError(
            InternalServerError {},
        )),
    }
}

fn abort_quest_accepted() -> AbortQuestResponse {
    AbortQuestResponse {
        response: Some(abort_quest_response::Response::Accepted(
            abort_quest_response::Accepted {},
        )),
    }
}
fn abort_quest_not_found() -> AbortQuestResponse {
    AbortQuestResponse {
        response: Some(abort_quest_response::Response::NotFoundQuestInstance(
            NotFoundQuestInstance {},
        )),
    }
}
fn abort_quest_not_owner() -> AbortQuestResponse {
    AbortQuestResponse {
        response: Some(abort_quest_response::Response::NotOwner(NotOwner {})),
    }
}
fn abort_quest_not_uuid() -> AbortQuestResponse {
    AbortQuestResponse {
        response: Some(abort_quest_response::Response::NotUuidError(NotUuid {})),
    }
}
fn abort_quest_internal_error() -> AbortQuestResponse {
    AbortQuestResponse {
        response: Some(abort_quest_response::Response::InternalServerError(
            InternalServerError {},
        )),
    }
}

fn event_accepted(event_id: &str) -> EventResponse {
    EventResponse {
        response: Some(event_response::Response::AcceptedEventId(
            event_id.to_string(),
        )),
    }
}
fn event_ignored() -> EventResponse {
    EventResponse {
        response: Some(event_response::Response::IgnoredEvent(IgnoredEvent {})),
    }
}
fn event_internal_error() -> EventResponse {
    EventResponse {
        response: Some(event_response::Response::InternalServerError(
            InternalServerError {},
        )),
    }
}

fn get_all_quests_ok(instances: Vec<QuestInstance>) -> GetAllQuestsResponse {
    GetAllQuestsResponse {
        response: Some(get_all_quests_response::Response::Quests(Quests {
            instances,
        })),
    }
}
fn get_all_quests_internal_error() -> GetAllQuestsResponse {
    GetAllQuestsResponse {
        response: Some(get_all_quests_response::Response::InternalServerError(
            InternalServerError {},
        )),
    }
}

fn get_quest_definition_ok(quest: Quest) -> GetQuestDefinitionResponse {
    GetQuestDefinitionResponse {
        response: Some(get_quest_definition_response::Response::Quest(quest)),
    }
}
fn get_quest_definition_internal_error() -> GetQuestDefinitionResponse {
    GetQuestDefinitionResponse {
        response: Some(
            get_quest_definition_response::Response::InternalServerError(InternalServerError {}),
        ),
    }
}

// ---- ServiceError (port of upstream rpc/service.rs ServiceError) ----

pub enum ServiceError {
    NotExistsTransportID,
    InternalError,
}

impl RemoteErrorResponse for ServiceError {
    fn error_code(&self) -> u32 {
        match self {
            Self::NotExistsTransportID => 1,
            Self::InternalError => 2,
        }
    }
    fn error_message(&self) -> String {
        match self {
            Self::NotExistsTransportID => "Not exists transport id".to_string(),
            Self::InternalError => "Internal error".to_string(),
        }
    }
}
