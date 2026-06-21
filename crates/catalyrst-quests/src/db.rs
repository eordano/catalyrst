//! Postgres layer for catalyrst-quests — faithful port of
//! `decentraland/quests` crates/db (SQL queries) returning the generated
//! protobuf types decoded from the stored `definition`/`event` blobs.

use chrono::NaiveDateTime;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use uuid::Uuid;

use crate::proto::{ProtocolMessage, Quest, QuestDefinition};

const SCHEMA: &str = include_str!("../migrations/0001_quests.sql");

/// A quest row as stored, mirroring upstream `StoredQuest`. `definition` is the
/// raw protobuf blob; `created_at` is unix seconds.
#[derive(Debug, Clone)]
pub struct StoredQuest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub definition: Vec<u8>,
    pub creator_address: String,
    pub image_url: String,
    pub active: bool,
    pub created_at: i64,
}

/// A quest instance row, mirroring upstream `QuestInstance`.
#[derive(Debug, Clone)]
pub struct QuestInstance {
    pub id: String,
    pub quest_id: String,
    pub user_address: String,
    pub start_timestamp: i64,
}

/// A stored event row, mirroring upstream `Event` (db::core::definitions).
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: String,
    pub user_address: String,
    pub quest_instance_id: String,
    pub timestamp: i64,
    pub event: Vec<u8>,
}

/// A reward item, mirroring upstream `QuestRewardItem` (camelCase imageLink).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewardItem {
    pub name: String,
    pub image_link: String,
}

/// A reward hook, mirroring upstream `QuestRewardHook`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewardHook {
    pub webhook_url: String,
    pub request_body: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("not found")]
    NotFound,
    #[error("invalid uuid: {0}")]
    NotUuid(String),
    #[error("definition decode failed")]
    DefinitionDecode,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub type DbResult<T> = Result<T, DbError>;

fn parse_uuid(s: &str) -> DbResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| DbError::NotUuid(s.to_string()))
}

fn date_to_unix(dt: NaiveDateTime) -> i64 {
    dt.and_utc().timestamp()
}

pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        let db = Self { pool };
        db.ensure_schema().await?;
        Ok(db)
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        sqlx::raw_sql(SCHEMA).execute(&self.pool).await?;
        Ok(())
    }

    fn row_to_stored_quest(row: &sqlx::postgres::PgRow, active: bool) -> DbResult<StoredQuest> {
        let id: Uuid = row.try_get("id")?;
        let created_at: NaiveDateTime = row.try_get("created_at")?;
        Ok(StoredQuest {
            id: id.to_string(),
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            definition: row.try_get("definition")?,
            creator_address: row.try_get("creator_address")?,
            image_url: row.try_get("image_url")?,
            active,
            created_at: date_to_unix(created_at),
        })
    }

    fn row_to_instance(row: &sqlx::postgres::PgRow) -> DbResult<QuestInstance> {
        let id: Uuid = row.try_get("id")?;
        let quest_id: Uuid = row.try_get("quest_id")?;
        let start: NaiveDateTime = row.try_get("start_timestamp")?;
        Ok(QuestInstance {
            id: id.to_string(),
            quest_id: quest_id.to_string(),
            user_address: row.try_get("user_address")?,
            start_timestamp: date_to_unix(start),
        })
    }

    // ---- Quests (read) ----

    /// `get_active_quests`: quests not in deactivated_quests, paged.
    pub async fn get_active_quests(&self, offset: i64, limit: i64) -> DbResult<Vec<StoredQuest>> {
        let rows = sqlx::query(
            "SELECT * FROM quests \
             WHERE id NOT IN (SELECT quest_id AS id FROM deactivated_quests) \
             OFFSET $1 LIMIT $2",
        )
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| Self::row_to_stored_quest(r, true))
            .collect()
    }

    pub async fn count_active_quests(&self) -> DbResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(id) FROM quests \
             WHERE id NOT IN (SELECT quest_id AS id FROM deactivated_quests)",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    /// `get_quest`: a single stored quest by id, with its computed `active`.
    pub async fn get_stored_quest(&self, id: &str) -> DbResult<StoredQuest> {
        let uuid = parse_uuid(id)?;
        let row = sqlx::query(
            "SELECT q.*, (CASE WHEN dq.quest_id IS NULL THEN true ELSE false END) AS active \
             FROM quests q LEFT JOIN deactivated_quests dq ON q.id = dq.quest_id \
             WHERE q.id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(DbError::NotFound)?;
        let active: bool = row.try_get("active")?;
        Self::row_to_stored_quest(&row, active)
    }

    /// `get_quests_by_creator_address`: a creator's quests that were not
    /// superseded by an update (uq.id IS NULL), newest first, with `active`.
    pub async fn get_quests_by_creator(
        &self,
        creator: &str,
        offset: i64,
        limit: i64,
    ) -> DbResult<Vec<StoredQuest>> {
        let rows = sqlx::query(
            "SELECT q.*, (CASE WHEN dq.quest_id IS NULL THEN true ELSE false END) AS active \
             FROM quests q \
             LEFT JOIN deactivated_quests dq ON q.id = dq.quest_id \
             LEFT JOIN quest_updates uq ON q.id = uq.previous_quest_id \
             WHERE q.creator_address = $1 AND uq.id IS NULL \
             ORDER BY created_at DESC OFFSET $2 LIMIT $3",
        )
        .bind(creator)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let active: bool = r.try_get("active")?;
                Self::row_to_stored_quest(r, active)
            })
            .collect()
    }

    pub async fn count_quests_by_creator(&self, creator: &str) -> DbResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(q.id) FROM quests q \
             LEFT JOIN quest_updates uq ON q.id = uq.previous_quest_id \
             WHERE q.creator_address = $1 AND uq.id IS NULL",
        )
        .bind(creator)
        .fetch_one(&self.pool)
        .await?)
    }

    /// `is_active_quest`: quest exists and is not deactivated.
    pub async fn is_active_quest(&self, quest_id: &str) -> DbResult<bool> {
        let uuid = parse_uuid(quest_id)?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM quests \
             WHERE id = $1 AND id NOT IN (SELECT quest_id AS id FROM deactivated_quests WHERE quest_id = $1))",
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?)
    }

    /// `is_quest_creator`: the quest exists and `creator_address` matches.
    pub async fn is_quest_creator(&self, quest_id: &str, creator: &str) -> DbResult<bool> {
        let uuid = parse_uuid(quest_id)?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM quests WHERE id = $1 AND creator_address = $2)",
        )
        .bind(uuid)
        .bind(creator)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Decode the protobuf definition for a quest id into a full `Quest`
    /// (upstream `get_quest_with_decoded_definition`).
    pub async fn get_quest_with_decoded_definition(&self, quest_id: &str) -> DbResult<Quest> {
        let stored = self.get_stored_quest(quest_id).await?;
        let definition = QuestDefinition::decode(stored.definition.as_slice())
            .map_err(|_| DbError::DefinitionDecode)?;
        Ok(Quest {
            id: stored.id,
            name: stored.name,
            description: stored.description,
            creator_address: stored.creator_address,
            definition: Some(definition),
            image_url: stored.image_url,
            active: stored.active,
            created_at: stored.created_at as u32,
        })
    }

    // ---- Instances (read + write) ----

    /// `get_quest_instance`.
    pub async fn get_quest_instance(&self, id: &str) -> DbResult<QuestInstance> {
        let uuid = parse_uuid(id)?;
        let row = sqlx::query("SELECT * FROM quest_instances WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(DbError::NotFound)?;
        Self::row_to_instance(&row)
    }

    /// `has_active_quest_instance`: user already has a non-abandoned instance.
    pub async fn has_active_quest_instance(&self, user: &str, quest_id: &str) -> DbResult<bool> {
        let uuid = parse_uuid(quest_id)?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM quest_instances \
             WHERE user_address = $1 AND quest_id = $2 \
             AND id NOT IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances))",
        )
        .bind(user)
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?)
    }

    /// `start_quest`: insert a new instance, returning its id.
    pub async fn start_quest(&self, quest_id: &str, user_address: &str) -> DbResult<String> {
        let quest_uuid = parse_uuid(quest_id)?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO quest_instances (id, quest_id, user_address) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(quest_uuid)
            .bind(user_address)
            .execute(&self.pool)
            .await?;
        Ok(id.to_string())
    }

    /// `abandon_quest_instance`.
    pub async fn abandon_quest_instance(&self, instance_id: &str) -> DbResult<String> {
        let instance_uuid = parse_uuid(instance_id)?;
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO abandoned_quest_instances (id, quest_instance_id) VALUES ($1, $2)",
        )
        .bind(id)
        .bind(instance_uuid)
        .execute(&self.pool)
        .await?;
        Ok(id.to_string())
    }

    /// `complete_quest_instance`.
    pub async fn complete_quest_instance(&self, instance_id: &str) -> DbResult<String> {
        let instance_uuid = parse_uuid(instance_id)?;
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO completed_quest_instances (id, quest_instance_id) VALUES ($1, $2)",
        )
        .bind(id)
        .bind(instance_uuid)
        .execute(&self.pool)
        .await?;
        Ok(id.to_string())
    }

    /// `get_active_user_quest_instances`: a user's non-abandoned instances.
    pub async fn get_active_user_quest_instances(
        &self,
        user: &str,
    ) -> DbResult<Vec<QuestInstance>> {
        let rows = sqlx::query(
            "SELECT * FROM quest_instances \
             WHERE user_address = $1 \
             AND id NOT IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances)",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(Self::row_to_instance).collect()
    }

    /// `get_active_quest_instances_by_quest_id`: paged non-abandoned instances.
    pub async fn get_active_quest_instances_by_quest_id(
        &self,
        quest_id: &str,
        offset: i64,
        limit: i64,
    ) -> DbResult<Vec<QuestInstance>> {
        let uuid = parse_uuid(quest_id)?;
        let rows = sqlx::query(
            "SELECT * FROM quest_instances \
             WHERE quest_id = $1 \
             AND id NOT IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances) \
             OFFSET $2 LIMIT $3",
        )
        .bind(uuid)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(Self::row_to_instance).collect()
    }

    pub async fn count_active_quest_instances_by_quest_id(&self, quest_id: &str) -> DbResult<i64> {
        let uuid = parse_uuid(quest_id)?;
        Ok(sqlx::query_scalar(
            "SELECT count(id) FROM quest_instances \
             WHERE quest_id = $1 \
             AND id NOT IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances)",
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?)
    }

    // ---- Events ----

    /// `add_event`: persist a protobuf-encoded event against an instance.
    pub async fn add_event(
        &self,
        event_id: &str,
        user_address: &str,
        event: &[u8],
        instance_id: &str,
    ) -> DbResult<()> {
        let id = parse_uuid(event_id)?;
        let instance_uuid = parse_uuid(instance_id)?;
        sqlx::query(
            "INSERT INTO events (id, user_address, event, quest_instance_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(user_address)
        .bind(event)
        .bind(instance_uuid)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// `get_events`: an instance's event log, oldest first.
    pub async fn get_events(&self, instance_id: &str) -> DbResult<Vec<StoredEvent>> {
        let uuid = parse_uuid(instance_id)?;
        let rows =
            sqlx::query("SELECT * FROM events WHERE quest_instance_id = $1 ORDER BY timestamp ASC")
                .bind(uuid)
                .fetch_all(&self.pool)
                .await?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("id")?;
                let instance: Uuid = r.try_get("quest_instance_id")?;
                let ts: NaiveDateTime = r.try_get("timestamp")?;
                Ok(StoredEvent {
                    id: id.to_string(),
                    user_address: r.try_get("user_address")?,
                    quest_instance_id: instance.to_string(),
                    timestamp: date_to_unix(ts),
                    event: r.try_get("event")?,
                })
            })
            .collect()
    }

    // ---- Rewards ----

    /// `get_quest_reward_items`: a quest's reward items (camelCase imageLink).
    pub async fn get_quest_reward_items(&self, quest_id: &str) -> DbResult<Vec<QuestRewardItem>> {
        let uuid = parse_uuid(quest_id)?;
        let rows = sqlx::query(
            "SELECT reward_name, reward_image FROM quest_reward_items WHERE quest_id = $1",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| QuestRewardItem {
                name: r.get("reward_name"),
                image_link: r.get("reward_image"),
            })
            .collect())
    }

    /// `get_quest_reward_hook`: a quest's reward webhook (creator-only path).
    pub async fn get_quest_reward_hook(&self, quest_id: &str) -> DbResult<QuestRewardHook> {
        let uuid = parse_uuid(quest_id)?;
        let row = sqlx::query(
            "SELECT webhook_url, request_body FROM quest_reward_hooks WHERE quest_id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(DbError::NotFound)?;
        Ok(QuestRewardHook {
            webhook_url: row.get("webhook_url"),
            request_body: row.try_get("request_body").ok(),
        })
    }
}
