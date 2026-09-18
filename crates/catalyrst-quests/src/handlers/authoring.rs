use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use catalyrst_commons::http::is_safe_http_url;

use crate::db::{
    CreateQuest, CreateReward, CreateRewardHook, CreateRewardItem, Db, ToggleOutcome, UpdateOutcome,
};
use crate::handlers::errors::QuestError;
use crate::handlers::signer_or_unauthorized;
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

/// Shape-only: `image_link` is rendered by the client and never fetched here, so the SSRF
/// guard applied to `webhook_url` would only reject hosts that legitimately serve art.
fn is_http_url_shape(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value.trim()) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| host.contains('.'))
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
            if !is_safe_http_url(&reward.hook.webhook_url) {
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
                .all(|item| is_http_url_shape(&item.image_link))
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
    let new_id = match db
        .update_quest(&quest_id, &body.to_create_quest(), &signer)
        .await?
    {
        UpdateOutcome::Updated(id) => id,
        UpdateOutcome::NotCreator => return Err(QuestError::NotQuestCreator),
        UpdateOutcome::NotUpdatable => return Err(QuestError::QuestIsNotUpdatable),
    };
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
    match db.deactivate_quest(&quest_id, &signer).await? {
        ToggleOutcome::Done => Ok(StatusCode::ACCEPTED),
        ToggleOutcome::NotCreator => Err(QuestError::NotQuestCreator),
        ToggleOutcome::NotApplicable => Err(QuestError::QuestIsCurrentlyDeactivated),
    }
}

pub async fn activate_quest(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/quests/{quest_id}/activate");
    let signer = signer_or_unauthorized(&headers, "put", &path).await?;
    let db = db_or_internal(&s)?;
    match db.activate_quest(&quest_id, &signer).await? {
        ToggleOutcome::Done => Ok(StatusCode::ACCEPTED),
        ToggleOutcome::NotCreator => Err(QuestError::NotQuestCreator),
        ToggleOutcome::NotApplicable => Err(QuestError::QuestNotActivable),
    }
}

pub async fn get_quest_stats(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, QuestError> {
    let path = format!("/api/quests/{quest_id}/stats");
    let signer = signer_or_unauthorized(&headers, "get", &path).await?;
    let db = db_or_internal(&s)?;
    let since = chrono::Utc::now().naive_utc() - chrono::Duration::hours(24);
    let stats = match db.quest_stats(&quest_id, &signer, since).await? {
        Some(stats) if stats.is_creator => stats,
        _ => return Err(QuestError::NotQuestCreator),
    };
    Ok(Json(GetQuestStatsResponse {
        active_players: stats.active as usize,
        abandoned: stats.abandoned as usize,
        completed: stats.completed as usize,
        started_in_last_24_hours: stats.started_since as usize,
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

#[cfg(test)]
mod tests {
    use super::{is_http_url_shape, is_safe_http_url};

    #[test]
    fn image_links_accept_any_dotted_http_host() {
        for url in [
            "https://cdn.example.com/img.png",
            "http://10.0.0.5/img.png",
            "https://assets.internal/img.png",
        ] {
            assert!(is_http_url_shape(url), "{url}");
        }
    }

    #[test]
    fn image_links_reject_non_urls_and_other_schemes() {
        for value in [
            "",
            "not a url",
            "see https://a.com/x here",
            "javascript:alert(1)",
            "ftp://cdn.example.com/img.png",
            "https://router/img.png",
        ] {
            assert!(!is_http_url_shape(value), "{value:?}");
        }
    }

    #[test]
    fn webhooks_keep_the_ssrf_guard_image_links_do_not() {
        assert!(!is_safe_http_url("http://10.0.0.5/hook"));
        assert!(is_safe_http_url("https://hooks.example.com/x"));
    }
}
