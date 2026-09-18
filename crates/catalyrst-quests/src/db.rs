use chrono::NaiveDateTime;
use sqlx::postgres::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::proto::{ProtocolMessage, Quest, QuestDefinition};

const SCHEMA: &str = include_str!("../migrations/0001_quests.sql");

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct QuestInstance {
    pub id: String,
    pub quest_id: String,
    pub user_address: String,
    pub start_timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct CreateRewardHook {
    pub webhook_url: String,
    pub request_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct CreateRewardItem {
    pub name: String,
    pub image_link: String,
}

#[derive(Debug, Clone)]
pub struct CreateReward {
    pub hook: CreateRewardHook,
    pub items: Vec<CreateRewardItem>,
}

#[derive(Debug, Clone)]
pub struct CreateQuest {
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub definition: Vec<u8>,
    pub reward: Option<CreateReward>,
}

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: String,
    pub user_address: String,
    pub quest_instance_id: String,
    pub timestamp: i64,
    pub event: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewardItem {
    pub name: String,
    pub image_link: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewardHook {
    pub webhook_url: String,
    pub request_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct QuestReward {
    pub items: Vec<QuestRewardItem>,
    pub hook: Option<QuestRewardHook>,
    pub is_creator: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct QuestStats {
    pub is_creator: bool,
    pub active: i64,
    pub abandoned: i64,
    pub completed: i64,
    pub started_since: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    Updated(String),
    NotCreator,
    NotUpdatable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleOutcome {
    Done,
    NotCreator,
    NotApplicable,
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
        let pool = catalyrst_db::connect_pool(
            url,
            &catalyrst_db::PoolSettings {
                max_connections: 5,
                idle_timeout_secs: 600,
                ..catalyrst_db::PoolSettings::default()
            },
        )
        .await?;
        let db = Self { pool };
        db.ensure_schema().await?;
        Ok(db)
    }

    pub async fn from_pool(pool: PgPool) -> anyhow::Result<Self> {
        let db = Self { pool };
        db.ensure_schema().await?;
        Ok(db)
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
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

    pub async fn get_active_quests_page(
        &self,
        offset: i64,
        limit: i64,
    ) -> DbResult<(Vec<StoredQuest>, i64)> {
        let rows = sqlx::query(
            "SELECT *, count(*) OVER() AS total_count FROM quests \
             WHERE id NOT IN (SELECT quest_id AS id FROM deactivated_quests) \
             OFFSET $1 LIMIT $2",
        )
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let total = match rows.first() {
            Some(r) => r.try_get("total_count")?,
            None if offset > 0 || limit <= 0 => self.count_active_quests().await?,
            None => 0,
        };
        let quests = rows
            .iter()
            .map(|r| Self::row_to_stored_quest(r, true))
            .collect::<DbResult<Vec<_>>>()?;
        Ok((quests, total))
    }

    pub async fn count_active_quests(&self) -> DbResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(id) FROM quests \
             WHERE id NOT IN (SELECT quest_id AS id FROM deactivated_quests)",
        )
        .fetch_one(&self.pool)
        .await?)
    }

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

    pub async fn get_quests_by_creator_page(
        &self,
        creator: &str,
        offset: i64,
        limit: i64,
    ) -> DbResult<(Vec<StoredQuest>, i64)> {
        let rows = sqlx::query(
            "SELECT q.*, (CASE WHEN dq.quest_id IS NULL THEN true ELSE false END) AS active, \
                    count(*) OVER() AS total_count \
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
        let total = match rows.first() {
            Some(r) => r.try_get("total_count")?,
            None if offset > 0 || limit <= 0 => self.count_quests_by_creator(creator).await?,
            None => 0,
        };
        let quests = rows
            .iter()
            .map(|r| {
                let active: bool = r.try_get("active")?;
                Self::row_to_stored_quest(r, active)
            })
            .collect::<DbResult<Vec<_>>>()?;
        Ok((quests, total))
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

    pub async fn get_quest_with_decoded_definition(&self, quest_id: &str) -> DbResult<Quest> {
        decode_quest(self.get_stored_quest(quest_id).await?)
    }

    pub async fn get_quest_instance_with_quest(
        &self,
        id: &str,
    ) -> DbResult<(QuestInstance, StoredQuest)> {
        let uuid = parse_uuid(id)?;
        let row = sqlx::query(
            "SELECT i.id, i.quest_id, i.user_address, i.start_timestamp, \
                    q.name, q.description, q.definition, q.creator_address, q.image_url, \
                    q.created_at, (dq.quest_id IS NULL) AS active \
             FROM quest_instances i \
             JOIN quests q ON q.id = i.quest_id \
             LEFT JOIN deactivated_quests dq ON dq.quest_id = q.id \
             WHERE i.id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(DbError::NotFound)?;
        let instance = Self::row_to_instance(&row)?;
        let created_at: NaiveDateTime = row.try_get("created_at")?;
        let quest = StoredQuest {
            id: instance.quest_id.clone(),
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            definition: row.try_get("definition")?,
            creator_address: row.try_get("creator_address")?,
            image_url: row.try_get("image_url")?,
            active: row.try_get("active")?,
            created_at: date_to_unix(created_at),
        };
        Ok((instance, quest))
    }

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

    pub async fn get_active_quest_instances_by_quest_id_page(
        &self,
        quest_id: &str,
        offset: i64,
        limit: i64,
    ) -> DbResult<(Vec<QuestInstance>, i64)> {
        let uuid = parse_uuid(quest_id)?;
        let rows = sqlx::query(
            "SELECT *, count(*) OVER() AS total_count FROM quest_instances \
             WHERE quest_id = $1 \
             AND id NOT IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances) \
             OFFSET $2 LIMIT $3",
        )
        .bind(uuid)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let total = match rows.first() {
            Some(r) => r.try_get("total_count")?,
            None if offset > 0 || limit <= 0 => {
                self.count_active_quest_instances_by_quest_id(quest_id)
                    .await?
            }
            None => 0,
        };
        let instances = rows
            .iter()
            .map(Self::row_to_instance)
            .collect::<DbResult<Vec<_>>>()?;
        Ok((instances, total))
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

    pub async fn get_quest_reward(
        &self,
        quest_id: &str,
        creator: Option<&str>,
    ) -> DbResult<QuestReward> {
        let uuid = parse_uuid(quest_id)?;
        let rows = sqlx::query(
            "SELECT i.reward_name, i.reward_image, h.webhook_url, h.request_body, \
                    (q.creator_address = $2) AS is_creator \
             FROM quest_reward_items i \
             LEFT JOIN quest_reward_hooks h ON h.quest_id = i.quest_id \
             LEFT JOIN quests q ON q.id = i.quest_id \
             WHERE i.quest_id = $1",
        )
        .bind(uuid)
        .bind(creator)
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        let mut hook = None;
        let mut is_creator = false;
        for r in &rows {
            items.push(QuestRewardItem {
                name: r.try_get("reward_name")?,
                image_link: r.try_get("reward_image")?,
            });
            if hook.is_none() {
                if let Some(webhook_url) = r.try_get::<Option<String>, _>("webhook_url")? {
                    hook = Some(QuestRewardHook {
                        webhook_url,
                        request_body: r.try_get("request_body").ok(),
                    });
                }
            }
            is_creator = r.try_get::<Option<bool>, _>("is_creator")?.unwrap_or(false);
        }
        Ok(QuestReward {
            items,
            hook,
            is_creator,
        })
    }

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

    pub async fn create_quest(&self, quest: &CreateQuest, creator: &str) -> DbResult<String> {
        let id = Uuid::new_v4();
        let reward = RewardBinds::from(quest.reward.as_ref());
        sqlx::query(
            "WITH nq AS ( \
                 INSERT INTO quests (id, name, description, definition, creator_address, image_url) \
                 VALUES ($1, $2, $3, $4, $5, $6) RETURNING id), \
             hook AS ( \
                 INSERT INTO quest_reward_hooks (quest_id, webhook_url, request_body) \
                 SELECT nq.id, $7, $8::json FROM nq WHERE $7::text IS NOT NULL), \
             items AS ( \
                 INSERT INTO quest_reward_items (quest_id, reward_name, reward_image) \
                 SELECT nq.id, t.n, t.i FROM nq, unnest($9::text[], $10::text[]) AS t(n, i)) \
             SELECT id FROM nq",
        )
        .bind(id)
        .bind(&quest.name)
        .bind(&quest.description)
        .bind(&quest.definition)
        .bind(creator)
        .bind(&quest.image_url)
        .bind(reward.webhook_url)
        .bind(reward.request_body)
        .bind(&reward.names)
        .bind(&reward.images)
        .execute(&self.pool)
        .await?;
        Ok(id.to_string())
    }

    /// Creator and not-yet-updated checks ride inside the statement so the
    /// caller can tell the three outcomes apart without pre-check round trips.
    pub async fn update_quest(
        &self,
        previous_quest_id: &str,
        quest: &CreateQuest,
        creator: &str,
    ) -> DbResult<UpdateOutcome> {
        let previous = parse_uuid(previous_quest_id)?;
        let quest_id = Uuid::new_v4();
        let reward = RewardBinds::from(quest.reward.as_ref());
        let row = sqlx::query(
            "WITH prev AS ( \
                 SELECT q.id, NOT EXISTS (SELECT 1 FROM quest_updates u \
                                          WHERE u.previous_quest_id = q.id) AS updatable \
                 FROM quests q WHERE q.id = $1 AND q.creator_address = $2), \
             nq AS ( \
                 INSERT INTO quests (id, name, description, definition, creator_address, image_url) \
                 SELECT $3, $4, $5, $6, $2, $7 FROM prev WHERE prev.updatable RETURNING id), \
             deact AS ( \
                 INSERT INTO deactivated_quests (id, quest_id) \
                 SELECT $8, prev.id FROM prev WHERE prev.updatable), \
             hook AS ( \
                 INSERT INTO quest_reward_hooks (quest_id, webhook_url, request_body) \
                 SELECT nq.id, $9, $10::json FROM nq WHERE $9::text IS NOT NULL), \
             items AS ( \
                 INSERT INTO quest_reward_items (quest_id, reward_name, reward_image) \
                 SELECT nq.id, t.n, t.i FROM nq, unnest($11::text[], $12::text[]) AS t(n, i)), \
             upd AS ( \
                 INSERT INTO quest_updates (id, quest_id, previous_quest_id) \
                 SELECT $13, nq.id, $1 FROM nq) \
             SELECT EXISTS (SELECT 1 FROM prev) AS is_creator, (SELECT id FROM nq) AS new_id",
        )
        .bind(previous)
        .bind(creator)
        .bind(quest_id)
        .bind(&quest.name)
        .bind(&quest.description)
        .bind(&quest.definition)
        .bind(&quest.image_url)
        .bind(Uuid::new_v4())
        .bind(reward.webhook_url)
        .bind(reward.request_body)
        .bind(&reward.names)
        .bind(&reward.images)
        .bind(Uuid::new_v4())
        .fetch_one(&self.pool)
        .await?;
        let is_creator: bool = row.try_get("is_creator")?;
        let new_id: Option<Uuid> = row.try_get("new_id")?;
        Ok(match (new_id, is_creator) {
            (Some(id), _) => UpdateOutcome::Updated(id.to_string()),
            (None, false) => UpdateOutcome::NotCreator,
            (None, true) => UpdateOutcome::NotUpdatable,
        })
    }

    pub async fn deactivate_quest(&self, quest_id: &str, creator: &str) -> DbResult<ToggleOutcome> {
        let uuid = parse_uuid(quest_id)?;
        let row = sqlx::query(
            "WITH q AS (SELECT id FROM quests WHERE id = $1 AND creator_address = $2), \
             ins AS ( \
                 INSERT INTO deactivated_quests (id, quest_id) \
                 SELECT $3, q.id FROM q \
                 WHERE NOT EXISTS (SELECT 1 FROM deactivated_quests d WHERE d.quest_id = q.id) \
                 RETURNING id) \
             SELECT EXISTS (SELECT 1 FROM q) AS is_creator, EXISTS (SELECT 1 FROM ins) AS done",
        )
        .bind(uuid)
        .bind(creator)
        .bind(Uuid::new_v4())
        .fetch_one(&self.pool)
        .await?;
        Self::toggle_outcome(&row)
    }

    pub async fn activate_quest(&self, quest_id: &str, creator: &str) -> DbResult<ToggleOutcome> {
        let uuid = parse_uuid(quest_id)?;
        let row = sqlx::query(
            "WITH q AS (SELECT id FROM quests WHERE id = $1 AND creator_address = $2), \
             c AS ( \
                 SELECT q.id FROM q \
                 WHERE EXISTS (SELECT 1 FROM deactivated_quests d WHERE d.quest_id = q.id) \
                   AND NOT EXISTS (SELECT 1 FROM quest_updates u WHERE u.previous_quest_id = q.id)), \
             del AS (DELETE FROM deactivated_quests WHERE quest_id IN (SELECT id FROM c) \
                     RETURNING quest_id) \
             SELECT EXISTS (SELECT 1 FROM q) AS is_creator, EXISTS (SELECT 1 FROM c) AS done",
        )
        .bind(uuid)
        .bind(creator)
        .fetch_one(&self.pool)
        .await?;
        Self::toggle_outcome(&row)
    }

    fn toggle_outcome(row: &sqlx::postgres::PgRow) -> DbResult<ToggleOutcome> {
        let is_creator: bool = row.try_get("is_creator")?;
        let done: bool = row.try_get("done")?;
        Ok(match (is_creator, done) {
            (_, true) => ToggleOutcome::Done,
            (false, false) => ToggleOutcome::NotCreator,
            (true, false) => ToggleOutcome::NotApplicable,
        })
    }

    pub async fn get_old_quest_versions(&self, quest_id: &str) -> DbResult<Vec<String>> {
        let rows = sqlx::query(
            "SELECT quest_id, previous_quest_id FROM quest_updates ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut versions = Vec::new();
        let mut cursor = quest_id.to_string();
        for row in &rows {
            let quest: Uuid = row.try_get("quest_id")?;
            let previous: Uuid = row.try_get("previous_quest_id")?;
            if quest.to_string() == cursor {
                versions.push(previous.to_string());
                cursor = previous.to_string();
            }
        }
        Ok(versions)
    }

    pub async fn quest_stats(
        &self,
        quest_id: &str,
        creator: &str,
        started_since: NaiveDateTime,
    ) -> DbResult<Option<QuestStats>> {
        let uuid = parse_uuid(quest_id)?;
        let row = sqlx::query(
            "SELECT (q.creator_address = $2) AS is_creator, \
                    count(i.id) FILTER (WHERE a.id IS NULL) AS active, \
                    count(i.id) FILTER (WHERE a.id IS NOT NULL) AS abandoned, \
                    count(i.id) FILTER (WHERE a.id IS NULL AND c.id IS NOT NULL) AS completed, \
                    count(i.id) FILTER (WHERE a.id IS NULL AND i.start_timestamp >= $3) \
                        AS started_since \
             FROM quests q \
             LEFT JOIN quest_instances i ON i.quest_id = q.id \
             LEFT JOIN abandoned_quest_instances a ON a.quest_instance_id = i.id \
             LEFT JOIN completed_quest_instances c ON c.quest_instance_id = i.id \
             WHERE q.id = $1 \
             GROUP BY q.id, q.creator_address",
        )
        .bind(uuid)
        .bind(creator)
        .bind(started_since)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(QuestStats {
                is_creator: r.try_get("is_creator")?,
                active: r.try_get("active")?,
                abandoned: r.try_get("abandoned")?,
                completed: r.try_get("completed")?,
                started_since: r.try_get("started_since")?,
            })
        })
        .transpose()
    }

    pub async fn remove_event(&self, event_id: &str, instance_id: &str) -> DbResult<()> {
        let event = parse_uuid(event_id)?;
        let instance = parse_uuid(instance_id)?;
        let removed: bool = sqlx::query_scalar(
            "WITH e AS (DELETE FROM events WHERE id = $1 RETURNING id), \
             c AS (DELETE FROM completed_quest_instances \
                   WHERE quest_instance_id = $2 AND EXISTS (SELECT 1 FROM e) RETURNING id) \
             SELECT EXISTS (SELECT 1 FROM e)",
        )
        .bind(event)
        .bind(instance)
        .fetch_one(&self.pool)
        .await?;
        if !removed {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    pub async fn reset_quest_instance(&self, instance_id: &str) -> DbResult<()> {
        let uuid = parse_uuid(instance_id)?;
        sqlx::query(
            "WITH e AS (DELETE FROM events WHERE quest_instance_id = $1 RETURNING id) \
             DELETE FROM completed_quest_instances WHERE quest_instance_id = $1",
        )
        .bind(uuid)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

pub fn decode_quest(stored: StoredQuest) -> DbResult<Quest> {
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

struct RewardBinds<'a> {
    webhook_url: Option<&'a str>,
    request_body: Option<sqlx::types::Json<&'a Option<serde_json::Value>>>,
    names: Vec<&'a str>,
    images: Vec<&'a str>,
}

impl<'a> From<Option<&'a CreateReward>> for RewardBinds<'a> {
    fn from(reward: Option<&'a CreateReward>) -> Self {
        match reward {
            Some(r) => Self {
                webhook_url: Some(r.hook.webhook_url.as_str()),
                request_body: Some(sqlx::types::Json(&r.hook.request_body)),
                names: r.items.iter().map(|i| i.name.as_str()).collect(),
                images: r.items.iter().map(|i| i.image_link.as_str()).collect(),
            },
            None => Self {
                webhook_url: None,
                request_body: None,
                names: Vec::new(),
                images: Vec::new(),
            },
        }
    }
}
