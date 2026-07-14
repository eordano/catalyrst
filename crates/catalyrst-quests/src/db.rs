use chrono::NaiveDateTime;
use sqlx::postgres::PgPool;
use sqlx::{QueryBuilder, Row};
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

    pub async fn get_quest_instance(&self, id: &str) -> DbResult<QuestInstance> {
        let uuid = parse_uuid(id)?;
        let row = sqlx::query("SELECT * FROM quest_instances WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(DbError::NotFound)?;
        Self::row_to_instance(&row)
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

    async fn insert_quest_row<'e, E>(exec: E, quest: &CreateQuest, creator: &str) -> DbResult<Uuid>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO quests (id, name, description, definition, creator_address, image_url) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(&quest.name)
        .bind(&quest.description)
        .bind(&quest.definition)
        .bind(creator)
        .bind(&quest.image_url)
        .execute(exec)
        .await?;
        Ok(id)
    }

    async fn insert_reward_hook<'e, E>(
        exec: E,
        quest_id: Uuid,
        hook: &CreateRewardHook,
    ) -> DbResult<()>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        sqlx::query(
            "INSERT INTO quest_reward_hooks (quest_id, webhook_url, request_body) \
             VALUES ($1, $2, $3)",
        )
        .bind(quest_id)
        .bind(&hook.webhook_url)
        .bind(sqlx::types::Json(&hook.request_body))
        .execute(exec)
        .await?;
        Ok(())
    }

    async fn insert_reward_items<'e, E>(
        exec: E,
        quest_id: Uuid,
        items: &[CreateRewardItem],
    ) -> DbResult<()>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let mut builder = QueryBuilder::new(
            "INSERT INTO quest_reward_items (quest_id, reward_name, reward_image)",
        );
        builder.push_values(items, |mut b, item| {
            b.push_bind(quest_id)
                .push_bind(&item.name)
                .push_bind(&item.image_link);
        });
        builder.build().execute(exec).await?;
        Ok(())
    }

    pub async fn create_quest(&self, quest: &CreateQuest, creator: &str) -> DbResult<String> {
        let mut tx = self.pool.begin().await?;
        let quest_id = Self::insert_quest_row(&mut *tx, quest, creator).await?;
        if let Some(reward) = &quest.reward {
            Self::insert_reward_hook(&mut *tx, quest_id, &reward.hook).await?;
            Self::insert_reward_items(&mut *tx, quest_id, &reward.items).await?;
        }
        tx.commit().await?;
        Ok(quest_id.to_string())
    }

    pub async fn update_quest(
        &self,
        previous_quest_id: &str,
        quest: &CreateQuest,
        creator: &str,
    ) -> DbResult<String> {
        let previous = parse_uuid(previous_quest_id)?;
        let mut tx = self.pool.begin().await?;
        let quest_id = Self::insert_quest_row(&mut *tx, quest, creator).await?;
        let deactivation_id = Uuid::new_v4();
        sqlx::query("INSERT INTO deactivated_quests (id, quest_id) VALUES ($1, $2)")
            .bind(deactivation_id)
            .bind(previous)
            .execute(&mut *tx)
            .await?;
        if let Some(reward) = &quest.reward {
            Self::insert_reward_hook(&mut *tx, quest_id, &reward.hook).await?;
            Self::insert_reward_items(&mut *tx, quest_id, &reward.items).await?;
        }
        let update_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO quest_updates (id, quest_id, previous_quest_id) VALUES ($1, $2, $3)",
        )
        .bind(update_id)
        .bind(quest_id)
        .bind(previous)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(quest_id.to_string())
    }

    pub async fn deactivate_quest(&self, quest_id: &str) -> DbResult<String> {
        let quest_uuid = parse_uuid(quest_id)?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO deactivated_quests (id, quest_id) VALUES ($1, $2)")
            .bind(id)
            .bind(quest_uuid)
            .execute(&self.pool)
            .await?;
        Ok(id.to_string())
    }

    pub async fn can_activate_quest(&self, quest_id: &str) -> DbResult<bool> {
        let uuid = parse_uuid(quest_id)?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM deactivated_quests \
             WHERE quest_id = $1 AND quest_id NOT IN \
             (SELECT previous_quest_id FROM quest_updates WHERE previous_quest_id = $1))",
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn activate_quest(&self, quest_id: &str) -> DbResult<bool> {
        let uuid = parse_uuid(quest_id)?;
        let result = sqlx::query("DELETE FROM deactivated_quests WHERE quest_id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() != 0)
    }

    pub async fn is_updatable(&self, quest_id: &str) -> DbResult<bool> {
        let uuid = parse_uuid(quest_id)?;
        let already_updated: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM quest_updates WHERE previous_quest_id = $1)",
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?;
        Ok(!already_updated)
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

    pub async fn get_all_quest_instances_by_quest_id(
        &self,
        quest_id: &str,
    ) -> DbResult<(Vec<QuestInstance>, Vec<QuestInstance>)> {
        let uuid = parse_uuid(quest_id)?;
        let rows = sqlx::query(
            "SELECT *, true AS active FROM quest_instances \
             WHERE quest_id = $1 \
             AND id NOT IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances) \
             UNION \
             SELECT *, false AS active FROM quest_instances \
             WHERE quest_id = $1 \
             AND id IN (SELECT quest_instance_id AS id FROM abandoned_quest_instances)",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await?;
        let mut actives = Vec::new();
        let mut abandoned = Vec::new();
        for row in &rows {
            let active: bool = row.try_get("active")?;
            let instance = Self::row_to_instance(row)?;
            if active {
                actives.push(instance);
            } else {
                abandoned.push(instance);
            }
        }
        Ok((actives, abandoned))
    }

    pub async fn is_completed_instance(&self, instance_id: &str) -> DbResult<bool> {
        let uuid = parse_uuid(instance_id)?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM completed_quest_instances WHERE quest_instance_id = $1)",
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn remove_event(&self, event_id: &str) -> DbResult<()> {
        let uuid = parse_uuid(event_id)?;
        let result = sqlx::query("DELETE FROM events WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    pub async fn remove_events_from_quest_instance(&self, instance_id: &str) -> DbResult<()> {
        let uuid = parse_uuid(instance_id)?;
        sqlx::query("DELETE FROM events WHERE quest_instance_id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn remove_instance_from_completed_instances(
        &self,
        instance_id: &str,
    ) -> DbResult<()> {
        let uuid = parse_uuid(instance_id)?;
        sqlx::query("DELETE FROM completed_quest_instances WHERE quest_instance_id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
