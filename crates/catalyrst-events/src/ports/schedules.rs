use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::schemas::ScheduleRecord;

pub struct SchedulesComponent {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct ScheduleLocalRow {
    id: String,
    name: String,
    description: Option<String>,
    image: Option<String>,
    theme: Option<String>,
    background: Value,
    active_since: Option<DateTime<Utc>>,
    active_until: Option<DateTime<Utc>>,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ScheduleLocalRow {
    fn into_record(self) -> ScheduleRecord {
        let background = self
            .background
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        ScheduleRecord {
            id: self.id,
            name: self.name,
            description: self.description,
            image: self.image,
            theme: self.theme,
            background,
            active_since: self.active_since,
            active_until: self.active_until,
            active: self.active,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
        }
    }
}

impl SchedulesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<ScheduleRecord>, ApiError> {
        let rows: Vec<ScheduleLocalRow> = sqlx::query_as(
            "SELECT id, name, description, image, theme, background, active_since, active_until, \
                    active, created_at, updated_at \
             FROM schedules_local ORDER BY active_since ASC NULLS LAST, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(ScheduleLocalRow::into_record)
            .collect())
    }

    pub async fn get(&self, schedule_id: &str) -> Result<Option<ScheduleRecord>, ApiError> {
        let row: Option<ScheduleLocalRow> = sqlx::query_as(
            "SELECT id, name, description, image, theme, background, active_since, active_until, \
                    active, created_at, updated_at \
             FROM schedules_local WHERE id = $1",
        )
        .bind(schedule_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ScheduleLocalRow::into_record))
    }

    pub async fn sitemap_schedule_ids(&self) -> Result<Vec<String>, ApiError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT s AS id \
             FROM event, jsonb_array_elements_text(raw->'schedules') AS s \
             WHERE jsonb_typeof(raw->'schedules') = 'array' \
             ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}
