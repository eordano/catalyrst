use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db::{CreateQuest, CreateReward, CreateRewardHook, CreateRewardItem, Db};
use crate::handlers::errors::QuestError;
use crate::handlers::{signer_or_unauthorized, url_is_valid};
use crate::proto::{ProtocolMessage, QuestDefinition};
use crate::validation::validate_definition;
use crate::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardHookInput {
    pub webhook_url: String,
    pub request_body: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardItemInput {
    pub name: String,
    pub image_link: String,
}

#[derive(Deserialize)]
pub struct QuestReward {
    pub hook: RewardHookInput,
    pub items: Vec<RewardItemInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuestRequest {
    pub name: String,
    pub description: String,
    pub definition: QuestDefinition,
    pub image_url: String,
    pub reward: Option<QuestReward>,
}

#[derive(Deserialize)]
#[serde(transparent)]
pub struct UpdateQuestRequest(pub CreateQuestRequest);

#[derive(Serialize)]
pub struct CreateQuestResponse {
    pub id: String,
}

#[derive(Serialize)]
pub struct UpdateQuestResponse {
    pub quest_id: String,
}

#[derive(Serialize)]
pub struct GetQuestStatsResponse {
    pub active_players: usize,
    pub abandoned: usize,
    pub completed: usize,
    pub started_in_last_24_hours: usize,
}

#[derive(Serialize)]
pub struct GetQuestUpdatesResponse {
    pub updates: Vec<String>,
}

impl CreateQuestRequest {
    fn validate(&self) -> Result<(), QuestError> {
        if self.name.trim().len() < 5 {
            return Err(QuestError::QuestValidation(
                "Name should be longer".to_string(),
            ));
        }
        if self.description.trim().len() < 5 {
            return Err(QuestError::QuestValidation(
                "Description should be longer".to_string(),
            ));
        }
        validate_definition(&self.definition)
            .map_err(|error| QuestError::QuestValidation(error.to_string()))?;

        if let Some(reward) = &self.reward {
            if !url_is_valid(&reward.hook.webhook_url) {
                return Err(QuestError::QuestValidation(
                    "Webhook url is not valid".to_string(),
                ));
            }
            if reward.items.is_empty() {
                return Err(QuestError::QuestValidation(
                    "Reward items must be at least one".to_string(),
                ));
            }
            if !reward
                .items
                .iter()
                .all(|item| url_is_valid(&item.image_link))
            {
                return Err(QuestError::QuestValidation(
                    "Item's image link is not valid".to_string(),
                ));
            }
            if !reward.items.iter().all(|item| item.name.len() >= 3) {
                return Err(QuestError::QuestValidation(
                    "Item name must be at least 3 characters".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn to_create_quest(&self) -> CreateQuest {
        let reward = self.reward.as_ref().map(|reward| CreateReward {
            hook: CreateRewardHook {
                webhook_url: reward.hook.webhook_url.clone(),
                request_body: reward
                    .hook
                    .request_body
                    .as_ref()
                    .and_then(|body| serde_json::to_value(body).ok()),
            },
            items: reward
                .items
                .iter()
                .map(|item| CreateRewardItem {
                    name: item.name.clone(),
                    image_link: item.image_link.clone(),
                })
                .collect(),
        });
        CreateQuest {
            name: self.name.clone(),
            description: self.description.clone(),
            image_url: self.image_url.clone(),
            definition: self.definition.encode_to_vec(),
            reward,
        }
    }
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

pub async fn create_quest(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateQuestRequest>,
) -> Result<impl IntoResponse, QuestError> {
    let signer = signer_or_unauthorized(&headers, "post", "/api/quests").await?;
    let db = db_or_internal(&s)?;
    body.validate()?;
    let id = db
        .create_quest(&body.to_create_quest(), &signer)
        .await
        .map_err(|_| QuestError::Internal)?;
    Ok((StatusCode::CREATED, Json(CreateQuestResponse { id })))
}

pub async fn update_quest(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateQuestRequest>,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/quests/{quest_id}");
    let signer = signer_or_unauthorized(&headers, "put", &path).await?;
    let db = db_or_internal(&s)?;
    let body = body.0;
    body.validate()?;
    require_quest_creator(db, &quest_id, &signer).await?;
    if !db.is_updatable(&quest_id).await? {
        return Err(QuestError::QuestIsNotUpdatable);
    }
    let new_id = db
        .update_quest(&quest_id, &body.to_create_quest(), &signer)
        .await?;
    Ok((
        StatusCode::OK,
        Json(UpdateQuestResponse { quest_id: new_id }),
    ))
}

pub async fn delete_quest(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/quests/{quest_id}");
    let signer = signer_or_unauthorized(&headers, "delete", &path).await?;
    let db = db_or_internal(&s)?;
    require_quest_creator(db, &quest_id, &signer).await?;
    if !db.is_active_quest(&quest_id).await? {
        return Err(QuestError::QuestIsCurrentlyDeactivated);
    }
    db.deactivate_quest(&quest_id).await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn activate_quest(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/quests/{quest_id}/activate");
    let signer = signer_or_unauthorized(&headers, "put", &path).await?;
    let db = db_or_internal(&s)?;
    require_quest_creator(db, &quest_id, &signer).await?;
    if !db.can_activate_quest(&quest_id).await? {
        return Err(QuestError::QuestNotActivable);
    }
    db.activate_quest(&quest_id).await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn get_quest_stats(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/quests/{quest_id}/stats");
    let signer = signer_or_unauthorized(&headers, "get", &path).await?;
    let db = db_or_internal(&s)?;
    require_quest_creator(db, &quest_id, &signer).await?;

    let (actives, abandoned) = db.get_all_quest_instances_by_quest_id(&quest_id).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut completed = 0usize;
    let mut started_in_last_24_hours = 0usize;
    for active in &actives {
        if now - active.start_timestamp <= 24 * 60 * 60 {
            started_in_last_24_hours += 1;
        }
        if db.is_completed_instance(&active.id).await? {
            completed += 1;
        }
    }
    Ok(Json(GetQuestStatsResponse {
        active_players: actives.len(),
        abandoned: abandoned.len(),
        completed,
        started_in_last_24_hours,
    }))
}

pub async fn get_quest_updates(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
) -> Result<impl IntoResponse, QuestError> {
    let db = db_or_internal(&s)?;
    let updates = db.get_old_quest_versions(&quest_id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(GetQuestUpdatesResponse { updates }),
    ))
}
