use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPool, Row};

#[derive(Debug, Clone)]
pub struct PriceOverride {
    pub token_id: String,
    pub vs_currency: String,
    pub value: String,
    pub note: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct OverridesComponent {
    pool: PgPool,
}

impl OverridesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<PriceOverride>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT token_id, vs_currency, value::text AS value, note, updated_by, updated_at \
             FROM price_overrides ORDER BY token_id, vs_currency",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_override).collect())
    }

    pub async fn set(
        &self,
        token_id: &str,
        vs_currency: &str,
        value: &str,
        note: Option<&str>,
        admin: &str,
    ) -> Result<PriceOverride, sqlx::Error> {
        let row = sqlx::query(
            "WITH o AS ( \
               INSERT INTO price_overrides (token_id, vs_currency, value, note, updated_by, updated_at) \
               VALUES ($1, $2, $3::numeric, $4, $5, NOW()) \
               ON CONFLICT (token_id, vs_currency) DO UPDATE \
                 SET value = EXCLUDED.value, \
                     note = EXCLUDED.note, \
                     updated_by = EXCLUDED.updated_by, \
                     updated_at = NOW() \
               RETURNING token_id, vs_currency, value::text AS value, note, updated_by, updated_at \
             ), a AS ( \
               INSERT INTO price_override_audit \
                   (action, token_id, vs_currency, value, note, admin, detail) \
               VALUES ('override.set', $1, $2, $3::numeric, $4, $5, $6) \
             ) \
             SELECT token_id, vs_currency, value, note, updated_by, updated_at FROM o",
        )
        .bind(token_id)
        .bind(vs_currency)
        .bind(value)
        .bind(note)
        .bind(admin)
        .bind(serde_json::json!({
            "token_id": token_id,
            "vs_currency": vs_currency,
            "value": value,
            "note": note,
        }))
        .fetch_one(&self.pool)
        .await?;
        Ok(row_to_override(row))
    }

    pub async fn clear(
        &self,
        token_id: &str,
        vs_currency: &str,
        admin: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "WITH d AS ( \
               DELETE FROM price_overrides WHERE token_id = $1 AND vs_currency = $2 \
               RETURNING token_id \
             ), a AS ( \
               INSERT INTO price_override_audit \
                   (action, token_id, vs_currency, value, note, admin, detail) \
               SELECT 'override.clear', $1, $2, NULL, NULL, $3, $4 FROM d \
             ) \
             SELECT EXISTS (SELECT 1 FROM d)",
        )
        .bind(token_id)
        .bind(vs_currency)
        .bind(admin)
        .bind(serde_json::json!({
            "token_id": token_id,
            "vs_currency": vs_currency,
        }))
        .fetch_one(&self.pool)
        .await
    }
}

fn row_to_override(r: sqlx::postgres::PgRow) -> PriceOverride {
    PriceOverride {
        token_id: r.get("token_id"),
        vs_currency: r.get("vs_currency"),
        value: r.get("value"),
        note: r.get("note"),
        updated_by: r.get("updated_by"),
        updated_at: r.get("updated_at"),
    }
}
