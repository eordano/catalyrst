use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db::{Db, QuestInstance};
use crate::handlers::errors::QuestError;
use crate::handlers::signer_or_unauthorized;
use crate::proto::EventRequest;
use crate::quests::build_event;
use crate::AppState;

#[derive(Serialize)]
pub struct GetQuestInstanceResponse {
    pub instance: QuestInstance,
}

#[derive(Deserialize)]
pub struct AddEventToInstancePayload {
    pub event: EventRequest,
}

#[derive(Serialize)]
pub struct AddEventToInstanceResponse {
    pub accepted: bool,
}

async fn require_quest_creator(db: &Db, quest_id: &str, signer: &str) -> Result<(), QuestError> {
    if !db.is_quest_creator(quest_id, signer).await? {
        return Err(QuestError::NotQuestCreator);
    }
    Ok(())
}

fn db_or_internal(s: &AppState) -> Result<&Db, QuestError> {
    s.db.as_deref().ok_or(QuestError::Internal)
}

pub async fn get_quest_instance(
    State(s): State<AppState>,
    Path(instance_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/instances/{instance_id}");
    let signer = signer_or_unauthorized(&headers, "get", &path).await?;
    let db = db_or_internal(&s)?;
    let instance = db.get_quest_instance(&instance_id).await?;
    require_quest_creator(db, &instance.quest_id, &signer).await?;
    Ok(Json(GetQuestInstanceResponse { instance }))
}

pub async fn add_event_to_instance(
    State(s): State<AppState>,
    Path(instance_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AddEventToInstancePayload>,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/instances/{instance_id}/events");
    let signer = signer_or_unauthorized(&headers, "post", &path).await?;
    let db = db_or_internal(&s)?;
    let instance = db.get_quest_instance(&instance_id).await?;
    require_quest_creator(db, &instance.quest_id, &signer).await?;

    let ctx = s.ctx.as_ref().ok_or(QuestError::Internal)?;
    let accepted = match build_event(&instance.user_address, payload.event) {
        Some((_id, event)) => ctx.push_event(event),
        None => false,
    };
    Ok(Json(AddEventToInstanceResponse { accepted }))
}

pub async fn remove_event_from_instance(
    State(s): State<AppState>,
    Path((instance_id, event_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/instances/{instance_id}/events/{event_id}");
    let signer = signer_or_unauthorized(&headers, "delete", &path).await?;
    let db = db_or_internal(&s)?;
    let instance = db.get_quest_instance(&instance_id).await?;
    require_quest_creator(db, &instance.quest_id, &signer).await?;
    db.remove_event(&event_id).await?;
    db.remove_instance_from_completed_instances(&instance.id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset_quest_instance(
    State(s): State<AppState>,
    Path(instance_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/instances/{instance_id}/reset");
    let signer = signer_or_unauthorized(&headers, "patch", &path).await?;
    let db = db_or_internal(&s)?;
    let instance = db.get_quest_instance(&instance_id).await?;
    let quest = db.get_stored_quest(&instance.quest_id).await?;
    if !signer.eq_ignore_ascii_case(&quest.creator_address) {
        return Err(QuestError::ResetQuestInstanceNotAllowed);
    }
    db.remove_events_from_quest_instance(&instance.id).await?;
    db.remove_instance_from_completed_instances(&instance.id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
