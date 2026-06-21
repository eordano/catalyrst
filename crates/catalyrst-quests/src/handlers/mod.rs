//! REST handlers — port of `decentraland/quests` crates/server/src/api/routes.
//! Every route is mounted under `/api` and returns the protobuf-defined message
//! shapes (camelCase via the generated serde attributes), byte-compatible with
//! upstream's actix routes.

use crate::auth_chain::optional_signer;
use crate::db::{Db, DbError, QuestRewardHook, QuestRewardItem};
use crate::proto::{Quest, QuestState};
use crate::state::{compute_instance_state_quest, get_state};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "catalyrst-quests" }))
}

#[derive(Deserialize)]
pub struct Page {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct GetQuestsResponse {
    quests: Vec<Quest>,
    total: i64,
}

#[derive(Serialize)]
pub struct GetQuestResponse {
    quest: Quest,
}

/// Decode a stored quest into a `Quest`, with `definition` set only when
/// `include_definition` (upstream gates this on creator identity).
fn to_quest(
    stored: &crate::db::StoredQuest,
    include_definition: bool,
) -> Result<Quest, StatusCode> {
    use crate::proto::{ProtocolMessage, QuestDefinition};
    let definition = if include_definition {
        Some(
            QuestDefinition::decode(stored.definition.as_slice())
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(Quest {
        id: stored.id.clone(),
        name: stored.name.clone(),
        description: stored.description.clone(),
        creator_address: stored.creator_address.clone(),
        definition,
        image_url: stored.image_url.clone(),
        active: stored.active,
        created_at: stored.created_at as u32,
    })
}

/// GET /api/quests — active quests; definitions are never included on the list.
pub async fn get_quests(
    State(s): State<AppState>,
    Query(p): Query<Page>,
) -> Result<Json<GetQuestsResponse>, StatusCode> {
    let Some(db) = s.db.as_deref() else {
        return Ok(Json(GetQuestsResponse {
            quests: vec![],
            total: 0,
        }));
    };
    let stored = db
        .get_active_quests(p.offset.unwrap_or(0), p.limit.unwrap_or(50))
        .await
        .map_err(internal)?;
    let total = db.count_active_quests().await.map_err(internal)?;
    let mut quests = Vec::with_capacity(stored.len());
    for sq in &stored {
        quests.push(to_quest(sq, false)?);
    }
    Ok(Json(GetQuestsResponse { quests, total }))
}

/// GET /api/quests/{quest_id} — definition included only when the signed-fetch
/// signer is the quest creator (upstream OptionalAuthUser gate).
pub async fn get_quest(
    State(s): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<GetQuestResponse>, StatusCode> {
    let Some(db) = s.db.as_deref() else {
        return Err(StatusCode::NOT_FOUND);
    };
    let stored = db
        .get_stored_quest(&id)
        .await
        .map_err(not_found_or_internal)?;
    let signer = optional_signer(&headers, "get", &format!("/api/quests/{id}"));
    let is_creator = signer
        .as_deref()
        .map(|a| a.eq_ignore_ascii_case(&stored.creator_address))
        .unwrap_or(false);
    let quest = to_quest(&stored, is_creator)?;
    Ok(Json(GetQuestResponse { quest }))
}

#[derive(Deserialize)]
pub struct RewardQuery {
    pub with_hook: Option<bool>,
}

#[derive(Serialize)]
pub struct GetQuestRewardResponse {
    items: Vec<QuestRewardItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hook: Option<QuestRewardHook>,
}

/// GET /api/quests/{quest_id}/reward — `{items:[{name,imageLink}], hook?}`.
/// The hook is included only when `with_hook=true` AND the signer is the quest
/// creator. 404 when the quest has no reward items (upstream QuestHasNoReward).
pub async fn get_quest_reward(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RewardQuery>,
    headers: HeaderMap,
) -> Result<Json<GetQuestRewardResponse>, StatusCode> {
    let Some(db) = s.db.as_deref() else {
        return Err(StatusCode::NOT_FOUND);
    };

    let mut with_hook = q.with_hook.unwrap_or(false);
    if with_hook {
        let signer = optional_signer(&headers, "get", &format!("/api/quests/{id}/reward"));
        let is_creator = match &signer {
            Some(addr) => db.is_quest_creator(&id, addr).await.map_err(internal)?,
            None => false,
        };
        if !is_creator {
            with_hook = false;
        }
    }

    if with_hook {
        let items = db.get_quest_reward_items(&id).await.map_err(internal)?;
        if items.is_empty() {
            return Err(StatusCode::NOT_FOUND);
        }
        let hook = db
            .get_quest_reward_hook(&id)
            .await
            .map_err(not_found_or_internal)?;
        Ok(Json(GetQuestRewardResponse {
            items,
            hook: Some(hook),
        }))
    } else {
        let items = db.get_quest_reward_items(&id).await.map_err(internal)?;
        if items.is_empty() {
            return Err(StatusCode::NOT_FOUND);
        }
        Ok(Json(GetQuestRewardResponse { items, hook: None }))
    }
}

/// GET /api/creators/{user_address}/quests — a creator's quests; definitions
/// included only when the authed signer IS that creator (upstream is_owner).
pub async fn get_quests_by_creator(
    State(s): State<AppState>,
    Path(creator): Path<String>,
    Query(p): Query<Page>,
    headers: HeaderMap,
) -> Result<Json<GetQuestsResponse>, StatusCode> {
    let Some(db) = s.db.as_deref() else {
        return Ok(Json(GetQuestsResponse {
            quests: vec![],
            total: 0,
        }));
    };
    let creator_lc = creator.to_ascii_lowercase();
    let signer = optional_signer(&headers, "get", &format!("/api/creators/{creator}/quests"));
    let is_owner = signer
        .as_deref()
        .map(|a| a.eq_ignore_ascii_case(&creator))
        .unwrap_or(false);

    let stored = db
        .get_quests_by_creator(&creator_lc, p.offset.unwrap_or(0), p.limit.unwrap_or(50))
        .await
        .map_err(internal)?;
    let total = db
        .count_quests_by_creator(&creator_lc)
        .await
        .map_err(internal)?;
    let mut quests = Vec::with_capacity(stored.len());
    for sq in &stored {
        quests.push(to_quest(sq, is_owner)?);
    }
    Ok(Json(GetQuestsResponse { quests, total }))
}

#[derive(Serialize)]
struct InstanceJson {
    id: String,
    #[serde(rename = "questId")]
    quest_id: String,
    #[serde(rename = "userAddress")]
    user_address: String,
    #[serde(rename = "startTimestamp")]
    start_timestamp: i64,
}

#[derive(Serialize)]
pub struct GetQuestInstancesResponse {
    instances: Vec<InstanceJson>,
    total: i64,
}

/// GET /api/quests/{quest_id}/instances — creator-gated list of active
/// instances (upstream RequiredAuthUser + is_quest_creator).
pub async fn get_quest_instances(
    State(s): State<AppState>,
    Path(quest_id): Path<String>,
    Query(p): Query<Page>,
    headers: HeaderMap,
) -> Result<Json<GetQuestInstancesResponse>, StatusCode> {
    let Some(db) = s.db.as_deref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let signer = require_creator(
        db,
        &quest_id,
        &headers,
        &format!("/api/quests/{quest_id}/instances"),
    )
    .await?;
    let _ = signer;

    let instances = db
        .get_active_quest_instances_by_quest_id(
            &quest_id,
            p.offset.unwrap_or(0),
            p.limit.unwrap_or(50),
        )
        .await
        .map_err(internal)?;
    let total = db
        .count_active_quest_instances_by_quest_id(&quest_id)
        .await
        .map_err(internal)?;
    Ok(Json(GetQuestInstancesResponse {
        instances: instances.into_iter().map(instance_json).collect(),
        total,
    }))
}

#[derive(Serialize)]
struct StoredEventJson {
    id: String,
    #[serde(rename = "userAddress")]
    user_address: String,
    #[serde(rename = "questInstanceId")]
    quest_instance_id: String,
    timestamp: i64,
    event: Vec<u8>,
}

#[derive(Serialize)]
pub struct GetInstanceStateResponse {
    state: QuestState,
    events: Vec<StoredEventJson>,
}

/// GET /api/instances/{quest_instance}/state — creator-gated current state +
/// raw event log (upstream get_quest_instance_state).
pub async fn get_instance_state(
    State(s): State<AppState>,
    Path(instance_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<GetInstanceStateResponse>, StatusCode> {
    let Some(db) = s.db.as_deref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let instance = db
        .get_quest_instance(&instance_id)
        .await
        .map_err(not_found_or_internal)?;
    require_creator(
        db,
        &instance.quest_id,
        &headers,
        &format!("/api/instances/{instance_id}/state"),
    )
    .await?;

    let quest = db
        .get_quest_with_decoded_definition(&instance.quest_id)
        .await
        .map_err(not_found_or_internal)?;
    let stored_events = db.get_events(&instance.id).await.map_err(internal)?;
    let decoded = {
        use crate::proto::ProtocolMessage;
        stored_events
            .iter()
            .filter_map(|e| crate::proto::Event::decode(e.event.as_slice()).ok())
            .collect::<Vec<_>>()
    };
    let state = get_state(&quest, &decoded);
    let events = stored_events
        .into_iter()
        .map(|e| StoredEventJson {
            id: e.id,
            user_address: e.user_address,
            quest_instance_id: e.quest_instance_id,
            timestamp: e.timestamp,
            event: e.event,
        })
        .collect();
    let _ = compute_instance_state_quest; // keep referenced for the RPC path
    Ok(Json(GetInstanceStateResponse { state, events }))
}

fn instance_json(i: crate::db::QuestInstance) -> InstanceJson {
    InstanceJson {
        id: i.id,
        quest_id: i.quest_id,
        user_address: i.user_address,
        start_timestamp: i.start_timestamp,
    }
}

/// Require the signed-fetch signer to be the creator of `quest_id` (upstream
/// RequiredAuthUser + is_quest_creator). 401 when unauthenticated, 403 when the
/// signer is not the creator.
async fn require_creator(
    db: &Db,
    quest_id: &str,
    headers: &HeaderMap,
    path: &str,
) -> Result<String, StatusCode> {
    let signer = optional_signer(headers, "get", path).ok_or(StatusCode::UNAUTHORIZED)?;
    let is_creator = db
        .is_quest_creator(quest_id, &signer)
        .await
        .map_err(internal)?;
    if !is_creator {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(signer)
}

fn internal(_e: DbError) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

fn not_found_or_internal(e: DbError) -> StatusCode {
    match e {
        DbError::NotFound => StatusCode::NOT_FOUND,
        DbError::NotUuid(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
