use sqlx::Row;

use crate::http::ApiError;
use crate::ports::credits::CreditsComponent;

#[derive(Debug, Clone)]
pub struct AuthorizationRow {
    pub id: String,
    pub address: String,
    pub usd_cents: i64,
    pub amount_wei: String,
    pub status: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct NewAuthorization<'a> {
    pub id: &'a str,
    pub address: &'a str,
    pub usd_cents: i64,
    pub amount_wei: &'a str,
    pub trade_id: Option<&'a str>,
    pub contract_address: Option<&'a str>,
    pub item_id: Option<&'a str>,
    pub source: Option<&'a str>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl CreditsComponent {
    pub async fn insert_authorization(&self, a: &NewAuthorization<'_>) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO credit_authorizations \
                 (id, address, usd_cents, amount_wei, trade_id, contract_address, \
                  item_id, source, status, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'authorized', $9)",
        )
        .bind(a.id)
        .bind(a.address)
        .bind(a.usd_cents)
        .bind(a.amount_wei)
        .bind(a.trade_id)
        .bind(a.contract_address)
        .bind(a.item_id)
        .bind(a.source)
        .bind(a.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn release_intents(&self, salts: &[String], address: &str) -> Result<u64, ApiError> {
        let res = sqlx::query(
            "UPDATE credit_authorizations SET status = 'released' \
             WHERE id = ANY($1) AND address = $2 AND status = 'authorized'",
        )
        .bind(salts)
        .bind(address)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn get_authorization(&self, id: &str) -> Result<Option<AuthorizationRow>, ApiError> {
        let row = sqlx::query(
            "SELECT id, address, usd_cents, amount_wei, status, expires_at \
             FROM credit_authorizations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| AuthorizationRow {
            id: r.get("id"),
            address: r.get("address"),
            usd_cents: r.get("usd_cents"),
            amount_wei: r.get("amount_wei"),
            status: r.get("status"),
            expires_at: r.get("expires_at"),
        }))
    }

    pub async fn usd_cents_to_mana_wei(
        &self,
        usd_cents: i64,
        mana_usd: &str,
    ) -> Result<(String, String), ApiError> {
        let row = sqlx::query(
            "SELECT floor(($1::numeric / 100) / $2::numeric * 1e18)::text AS amount_wei, \
                    floor($2::numeric * 1e18)::text AS oracle_rate",
        )
        .bind(usd_cents)
        .bind(mana_usd)
        .fetch_one(&self.pool)
        .await?;
        Ok((row.get("amount_wei"), row.get("oracle_rate")))
    }
}
