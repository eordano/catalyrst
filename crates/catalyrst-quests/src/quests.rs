//! Quest domain operations — port of `decentraland/quests`
//! crates/server/src/domain/{quests,events}.rs plus crates/system/src/rewards.rs.

use crate::db::{Db, DbError};
use crate::proto::EventRequest;
use crate::proto::{Action, Event, ProtocolMessage};
use crate::state::{get_state, is_completed, QuestGraph};
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

/// `start_quest`: the quest must be active and the user must not already have an
/// active instance of it; returns the new instance id.
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

/// `abandon_quest`: only the instance owner may abandon, and only if the quest
/// is not already completed.
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

/// Compute an instance's current `QuestState` by folding its event log against
/// the decoded quest definition (upstream `get_instance_state`).
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
    let _ = QuestGraph::from(&quest); // validate definition shape early
    Ok(get_state(&quest, &events))
}

/// `add_event_controller`: wrap a SendEvent action into an `Event` with a fresh
/// id and return it for queueing. Returns None when the request has no action
/// (upstream `AddEventError::NoAction`).
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

// ---- Rewards (port of crates/system/src/rewards.rs) ----

#[derive(Debug, serde::Deserialize)]
struct RewardsHookResponse {
    ok: bool,
}

/// `{user_address}` / `{quest_id}` placeholder substitution (port of
/// crates/protocol/src/rewards.rs::rewards_parser).
fn rewards_parser(to_be_parsed: &str, quest_id: &str, user_address: &str) -> String {
    to_be_parsed
        .replace("{user_address}", user_address)
        .replace("{quest_id}", quest_id)
}

/// Fire the quest's reward webhook on completion (port of `give_rewards_to_user`).
/// No-op when the quest has no reward hook.
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

async fn call_rewards_hook(
    url: &str,
    body: Option<serde_json::Value>,
    quest_id: &str,
    user_address: &str,
) -> Result<bool, String> {
    let url_parsed = rewards_parser(url, quest_id, user_address);
    let mut client = reqwest::Client::new().post(&url_parsed);

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
    let parsed = response
        .json::<RewardsHookResponse>()
        .await
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
