use crate::db::{Db, DbError};
use crate::proto::EventRequest;
use crate::proto::{Action, Event, ProtocolMessage};
use crate::state::{get_state, is_completed, QuestGraph};
use catalyrst_commons::http::{http_client, is_safe_http_url, HttpClientCfg};
use futures::StreamExt;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum QuestError {
    #[error("Quest doesn't exist or is inactive")]
    NotFoundOrInactive,
    #[error("Quest already started and active")]
    QuestAlreadyStarted,
    #[error("Cannot modify a quest instance if you are not the user playing the quest")]
    NotInstanceOwner,
    #[error("Quest already completed")]
    QuestAlreadyCompleted,
    #[error("not a UUID")]
    NotUuid,
    #[error("not found")]
    NotFound,
    #[error("internal: {0}")]
    Internal(String),
}

impl From<DbError> for QuestError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::NotFound => QuestError::NotFound,
            DbError::NotUuid(_) => QuestError::NotUuid,
            other => QuestError::Internal(other.to_string()),
        }
    }
}

pub async fn start_quest(
    db: &Db,
    user_address: &str,
    quest_id: &str,
) -> Result<String, QuestError> {
    if !db.is_active_quest(quest_id).await? {
        return Err(QuestError::NotFoundOrInactive);
    }
    if db.has_active_quest_instance(user_address, quest_id).await? {
        return Err(QuestError::QuestAlreadyStarted);
    }
    Ok(db.start_quest(quest_id, user_address).await?)
}

pub async fn abandon_quest(
    db: &Db,
    user_address: &str,
    quest_instance_id: &str,
) -> Result<(), QuestError> {
    let instance = db.get_quest_instance(quest_instance_id).await?;
    if instance.user_address != user_address {
        return Err(QuestError::NotInstanceOwner);
    }

    let state = compute_instance_state(db, &instance.quest_id, &instance.id).await?;
    if is_completed(&state) {
        return Err(QuestError::QuestAlreadyCompleted);
    }

    db.abandon_quest_instance(quest_instance_id).await?;
    Ok(())
}

pub async fn compute_instance_state(
    db: &Db,
    quest_id: &str,
    instance_id: &str,
) -> Result<crate::proto::QuestState, QuestError> {
    let quest = db.get_quest_with_decoded_definition(quest_id).await?;
    let stored_events = db.get_events(instance_id).await?;
    let events: Vec<Event> = stored_events
        .iter()
        .filter_map(|e| Event::decode(e.event.as_slice()).ok())
        .collect();
    let _ = QuestGraph::from(&quest);
    Ok(get_state(&quest, &events))
}

pub fn build_event(user_address: &str, request: EventRequest) -> Option<(Uuid, Event)> {
    let action: Action = request.action?;
    let id = Uuid::new_v4();
    let event = Event {
        id: id.to_string(),
        address: user_address.to_string(),
        action: Some(action),
    };
    Some((id, event))
}

#[derive(Debug, serde::Deserialize)]
struct RewardsHookResponse {
    ok: bool,
}

fn rewards_parser(to_be_parsed: &str, quest_id: &str, user_address: &str) -> String {
    to_be_parsed
        .replace("{user_address}", user_address)
        .replace("{quest_id}", quest_id)
}

pub async fn give_rewards_to_user(db: &Db, quest_id: &str, user_address: &str) {
    let hook = match db.get_quest_reward_hook(quest_id).await {
        Ok(h) => h,
        Err(DbError::NotFound) => {
            tracing::debug!("processing event > quest has no reward");
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "processing event > failed to get quest reward");
            return;
        }
    };
    if let Err(error) =
        call_rewards_hook(&hook.webhook_url, hook.request_body, quest_id, user_address).await
    {
        tracing::error!(%error, quest_id, user_address, "processing event > failed to assign reward");
    } else {
        tracing::info!(quest_id, user_address, "processing event > reward assigned");
    }
}

const REWARDS_HOOK_TIMEOUT: Duration = Duration::from_secs(10);
const REWARDS_HOOK_MAX_BODY_BYTES: usize = 64 * 1024;

fn rewards_hook_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        http_client(
            "quests-rewards-hook",
            &HttpClientCfg::default().with_total_timeout(REWARDS_HOOK_TIMEOUT),
        )
    })
}

async fn read_rewards_hook_body(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length() {
        if len > max_bytes as u64 {
            return Err("Rewards hook response too large".to_string());
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Couldn't read rewards hook response".to_string())?;
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err("Rewards hook response too large".to_string());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

async fn call_rewards_hook(
    url: &str,
    body: Option<serde_json::Value>,
    quest_id: &str,
    user_address: &str,
) -> Result<bool, String> {
    let url_parsed = rewards_parser(url, quest_id, user_address);
    if !is_safe_http_url(&url_parsed) {
        return Err("Rewards hook url is not safe to call".to_string());
    }
    let mut client = rewards_hook_client().post(&url_parsed);

    if let Some(serde_json::Value::Object(map)) = body {
        let parsed: serde_json::Map<String, serde_json::Value> = map
            .into_iter()
            .map(|(k, v)| {
                let v = match v {
                    serde_json::Value::String(s) => {
                        serde_json::Value::String(rewards_parser(&s, quest_id, user_address))
                    }
                    other => other,
                };
                (k, v)
            })
            .collect();
        client = client.json(&parsed);
    }

    let response = client
        .send()
        .await
        .map_err(|_| "Couldn't call rewards hook".to_string())?;
    let body_bytes = read_rewards_hook_body(response, REWARDS_HOOK_MAX_BODY_BYTES).await?;
    let parsed: RewardsHookResponse = serde_json::from_slice(&body_bytes)
        .map_err(|_| "Couldn't decode rewards hook response".to_string())?;
    Ok(parsed.ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewards_parser_substitutes() {
        assert_eq!(
            rewards_parser("http://h/{quest_id}/{user_address}", "123", "0xB"),
            "http://h/123/0xB"
        );
        assert_eq!(
            rewards_parser("http://h/quest_id", "123", "0xB"),
            "http://h/quest_id"
        );
    }
}
