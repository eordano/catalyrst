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
        self.reserve(a, Some(a.amount_wei), None, idempotency_key)
            .await
            .map(|(outcome, _)| outcome)
    }

    /// [`Self::reserve_authorization`] with the MANA amount derived in SQL from
    /// `a.usd_cents` at `mana_usd` (`a.amount_wei` is ignored); also returns the
    /// oracle rate in wei-per-MANA the way `usd_cents_to_mana_wei` did.
    pub async fn reserve_authorization_priced(
        &self,
        a: &NewAuthorization<'_>,
        mana_usd: &str,
        idempotency_key: &str,
    ) -> Result<(ReserveOutcome, String), ApiError> {
        let (outcome, rate) = self
            .reserve(a, None, Some(mana_usd), idempotency_key)
            .await?;
        let rate = rate.ok_or_else(|| ApiError::Internal("oracle rate missing".into()))?;
        Ok((outcome, rate))
    }

    /// Two statements after BEGIN: the wallet lock, then (under that lock, so in a fresh
    /// snapshot) the idempotency probe, the outstanding-authorized budget, the wei math and
    /// the conditional INSERT as one statement.
    async fn reserve(
        &self,
        a: &NewAuthorization<'_>,
        given_wei: Option<&str>,
        mana_usd: Option<&str>,
        idempotency_key: &str,
    ) -> Result<(ReserveOutcome, Option<String>), ApiError> {
        let mut tx = self.pool.begin().await?;

        let available_cents: Option<i64> = sqlx::query_scalar(
            "SELECT round(available * 100)::bigint FROM user_credits \
             WHERE address = $1 FOR UPDATE",
        )
        .bind(a.address)
        .fetch_optional(&mut *tx)
        .await?;

        let row = sqlx::query(
            "WITH prior AS ( \
                 SELECT id, amount_wei, usd_cents, expires_at \
                 FROM credit_authorizations WHERE idempotency_key = $10::text \
             ), budget AS ( \
                 SELECT $12::bigint - (SELECT COALESCE(SUM(usd_cents), 0)::bigint \
                                       FROM credit_authorizations \
                                       WHERE address = $2::text AND status = 'authorized' \
                                         AND expires_at > now()) AS remaining_cents, \
                        COALESCE($4::text, \
                                 floor(($3::bigint::numeric / 100) / $11::numeric * 1e18)::text) \
                            AS amount_wei, \
                        floor($11::numeric * 1e18)::text AS oracle_rate \
             ), ins AS ( \
                 INSERT INTO credit_authorizations \
                     (id, address, usd_cents, amount_wei, trade_id, contract_address, \
                      item_id, source, status, expires_at, idempotency_key) \
                 SELECT $1::text, $2::text, $3::bigint, b.amount_wei, $5::text, $6::text, \
                        $7::text, $8::text, 'authorized', $9::timestamptz, $10::text \
                 FROM budget b \
                 WHERE NOT EXISTS (SELECT 1 FROM prior) AND $3::bigint <= b.remaining_cents \
                 RETURNING id \
             ) \
             SELECT p.id AS prior_id, p.amount_wei AS prior_wei, p.usd_cents AS prior_cents, \
                    p.expires_at AS prior_expires, b.amount_wei, b.oracle_rate, \
                    EXISTS (SELECT 1 FROM ins) AS inserted \
             FROM budget b LEFT JOIN prior p ON TRUE",
        )
        .bind(a.id)
        .bind(a.address)
        .bind(a.usd_cents)
        .bind(given_wei)
        .bind(a.trade_id)
        .bind(a.contract_address)
        .bind(a.item_id)
        .bind(a.source)
        .bind(a.expires_at)
        .bind(idempotency_key)
        .bind(mana_usd)
        .bind(available_cents.unwrap_or(0))
        .fetch_one(&mut *tx)
        .await?;

        let oracle_rate: Option<String> = row.get("oracle_rate");
        if let Some(prior_id) = row.get::<Option<String>, _>("prior_id") {
            tx.commit().await?;
            return Ok((
                ReserveOutcome {
                    id: prior_id,
                    amount_wei: row.get("prior_wei"),
                    usd_cents: row.get("prior_cents"),
                    expires_at: row.get("prior_expires"),
                    replayed: true,
                },
                oracle_rate,
            ));
        }
        if !row.get::<bool, _>("inserted") {
            return Err(ApiError::payment_required("insufficient credit balance"));
        }
        let amount_wei: String = row
            .get::<Option<String>, _>("amount_wei")
            .ok_or_else(|| ApiError::Internal("authorization amount missing".into()))?;

        tx.commit().await?;
        Ok((
            ReserveOutcome {
                id: a.id.to_string(),
                amount_wei,
                usd_cents: a.usd_cents,
                expires_at: a.expires_at,
                replayed: false,
            },
            oracle_rate,
        ))
    }

    /// Flip live-but-past-expiry 'authorized' rows to 'expired' so they stop
    /// counting against the outstanding-authorized budget. The outbox worker tick
    /// inlines this same UPDATE (see `OutboxWorker::run_once`).
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
