use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPool, Row};

/// One manual price override row (docs/admin-console.md §4 "Price override").
///
/// `value` is an exact NUMERIC carried as a decimal string — never f64.
/// Mirrors the credits crate's never-f64 stance so an operator override keeps
/// full decimal precision and never round-trips through binary float.
#[derive(Debug, Clone)]
pub struct PriceOverride {
    pub token_id: String,
    pub vs_currency: String,
    pub value: String,
    pub note: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// The dynamic price-override config store. Reads are public (the store is a
/// read-only projection to unauthenticated callers); set/clear are bearer-gated
/// by the handler layer.
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

    /// Upsert an override for `(token_id, vs_currency)` and append an audit row.
    ///
    /// `value` is a validated decimal string bound as exact NUMERIC. `admin` is
    /// the console-attributed identity recorded in the audit trail. The override
    /// row and its audit row are written in one transaction so an override never
    /// lands without an attributed audit entry.
    pub async fn set(
        &self,
        token_id: &str,
        vs_currency: &str,
        value: &str,
        note: Option<&str>,
        admin: &str,
    ) -> Result<PriceOverride, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "INSERT INTO price_overrides (token_id, vs_currency, value, note, updated_by, updated_at) \
             VALUES ($1, $2, $3::numeric, $4, $5, NOW()) \
             ON CONFLICT (token_id, vs_currency) DO UPDATE \
               SET value = EXCLUDED.value, \
                   note = EXCLUDED.note, \
                   updated_by = EXCLUDED.updated_by, \
                   updated_at = NOW() \
             RETURNING token_id, vs_currency, value::text AS value, note, updated_by, updated_at",
        )
        .bind(token_id)
        .bind(vs_currency)
        .bind(value)
        .bind(note)
        .bind(admin)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO price_override_audit \
                 (action, token_id, vs_currency, value, note, admin, detail) \
             VALUES ('override.set', $1, $2, $3::numeric, $4, $5, $6)",
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
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row_to_override(row))
    }

    /// Delete the override for `(token_id, vs_currency)` and append an audit row.
    /// Returns true if a row was removed. The audit row is only written when a
    /// row was actually removed (a no-op clear is not recorded).
    pub async fn clear(
        &self,
        token_id: &str,
        vs_currency: &str,
        admin: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let res =
            sqlx::query("DELETE FROM price_overrides WHERE token_id = $1 AND vs_currency = $2")
                .bind(token_id)
                .bind(vs_currency)
                .execute(&mut *tx)
                .await?;
        let removed = res.rows_affected() > 0;
        if removed {
            sqlx::query(
                "INSERT INTO price_override_audit \
                     (action, token_id, vs_currency, value, note, admin, detail) \
                 VALUES ('override.clear', $1, $2, NULL, NULL, $3, $4)",
            )
            .bind(token_id)
            .bind(vs_currency)
            .bind(admin)
            .bind(serde_json::json!({
                "token_id": token_id,
                "vs_currency": vs_currency,
            }))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(removed)
    }
}

fn row_to_override(r: sqlx::postgres::PgRow) -> PriceOverride {
    // `value` is selected as `::text` so it arrives as the exact NUMERIC string
    // without any f64 intermediary.
    PriceOverride {
        token_id: r.get("token_id"),
        vs_currency: r.get("vs_currency"),
        value: r.get("value"),
        note: r.get("note"),
        updated_by: r.get("updated_by"),
        updated_at: r.get("updated_at"),
    }
}
