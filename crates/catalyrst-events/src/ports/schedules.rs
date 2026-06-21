use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::schemas::ScheduleRecord;

pub struct SchedulesComponent {
    pool: PgPool,
}

impl SchedulesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<ScheduleRecord>, ApiError> {
        Ok(Vec::new())
    }

    pub async fn get(&self, _schedule_id: &str) -> Result<Option<ScheduleRecord>, ApiError> {
        Ok(None)
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
