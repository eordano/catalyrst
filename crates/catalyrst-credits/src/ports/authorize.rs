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

#[derive(Debug, Clone)]
pub struct ReserveOutcome {
    pub id: String,
    pub amount_wei: String,
    pub usd_cents: i64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub replayed: bool,
}

impl CreditsComponent {
    /// Reserve credits for an on-chain authorization under a single
    /// transaction that row-locks the wallet balance and subtracts the sum of
    /// still-outstanding 'authorized' rows from the spendable balance.
    ///
    /// Without the lock, N concurrent calls each read the same unlocked
    /// balance, each pass the sufficiency check, and each mint a distinct
    /// signed claim for the full balance -- a treasury drain of N x balance.
    /// The `FOR UPDATE` on `user_credits` serializes callers for a wallet, and
    /// the outstanding-authorized SUM makes every prior live reservation count
    /// against the budget, so the pledged total can never exceed the balance.
    ///
    /// `idempotency_key` is derived server-side from the request; a replay
    /// returns the previously issued credit (id/amount/expiry) instead of
    /// minting a second claim. The existence probe runs AFTER the wallet lock,
    /// so two concurrent requests carrying the same key serialize and the
    /// second observes the first's committed row.
    pub async fn reserve_authorization(
        &self,
        a: &NewAuthorization<'_>,
        idempotency_key: &str,
    ) -> Result<ReserveOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;

        let available_cents: Option<i64> = sqlx::query_scalar(
            "SELECT round(available * 100)::bigint FROM user_credits \
             WHERE address = $1 FOR UPDATE",
        )
        .bind(a.address)
        .fetch_optional(&mut *tx)
        .await?;

        let prior = sqlx::query(
            "SELECT id, amount_wei, usd_cents, expires_at \
             FROM credit_authorizations WHERE idempotency_key = $1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(r) = prior {
            tx.commit().await?;
            return Ok(ReserveOutcome {
                id: r.get("id"),
                amount_wei: r.get("amount_wei"),
                usd_cents: r.get("usd_cents"),
                expires_at: r.get("expires_at"),
                replayed: true,
            });
        }

        let available_cents = available_cents.unwrap_or(0);
        let outstanding_cents: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(usd_cents), 0)::bigint FROM credit_authorizations \
             WHERE address = $1 AND status = 'authorized' AND expires_at > now()",
        )
        .bind(a.address)
        .fetch_one(&mut *tx)
        .await?;

        if a.usd_cents > available_cents - outstanding_cents {
            return Err(ApiError::payment_required("insufficient credit balance"));
        }

        sqlx::query(
            "INSERT INTO credit_authorizations \
                 (id, address, usd_cents, amount_wei, trade_id, contract_address, \
                  item_id, source, status, expires_at, idempotency_key) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'authorized', $9, $10)",
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
        .bind(idempotency_key)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(ReserveOutcome {
            id: a.id.to_string(),
            amount_wei: a.amount_wei.to_string(),
            usd_cents: a.usd_cents,
            expires_at: a.expires_at,
            replayed: false,
        })
    }

    /// Flip live-but-past-expiry 'authorized' rows to 'expired' so they stop
    /// counting against the outstanding-authorized budget. Runs on the standing
    /// worker cadence (see `OutboxWorker::run_once`).
    pub async fn expire_stale_authorizations(&self) -> Result<u64, ApiError> {
        let res = sqlx::query(
            "UPDATE credit_authorizations SET status = 'expired' \
             WHERE status = 'authorized' AND expires_at <= now()",
        )
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

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
