use sqlx::PgPool;

use crate::error::FedError;
use crate::sig::MAX_SKEW_PAST_SECS;

const REPLAY_RETENTION_SLACK_SECS: i64 = 60;

fn retention_expiry(signed_at: i64) -> i64 {
    signed_at + MAX_SKEW_PAST_SECS + REPLAY_RETENTION_SLACK_SECS
}

pub struct NonceReplayGuard {
    delete_sql: String,
    insert_sql: String,
}

impl NonceReplayGuard {
    pub fn new(table: &'static str) -> Self {
        Self {
            delete_sql: format!("DELETE FROM {table} WHERE expires_at < $1"),
            insert_sql: format!(
                "INSERT INTO {table} (signer, nonce, expires_at) VALUES ($1,$2,$3) \
                 ON CONFLICT (signer, nonce) DO NOTHING"
            ),
        }
    }

    pub async fn check_and_record(
        &self,
        pool: &PgPool,
        signer: &str,
        nonce: &[u8; 16],
        signed_at: i64,
    ) -> Result<(), FedError> {
        let signer = signer.to_ascii_lowercase();
        let nonce_hex = hex::encode(nonce);
        let expires_at = retention_expiry(signed_at);

        let now = chrono::Utc::now().timestamp();
        let _ = sqlx::query(sqlx::AssertSqlSafe(self.delete_sql.clone()))
            .bind(now)
            .execute(pool)
            .await;

        let res = sqlx::query(sqlx::AssertSqlSafe(self.insert_sql.clone()))
            .bind(&signer)
            .bind(&nonce_hex)
            .bind(expires_at)
            .execute(pool)
            .await
            .map_err(|e| FedError::Transport(e.to_string()))?;

        if res.rows_affected() == 0 {
            return Err(FedError::DuplicateNonce { signer });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sig::MAX_SKEW_FUTURE_SECS;

    #[test]
    fn retention_outlives_the_verify_acceptance_window() {
        let signed_at = 1_000_000;
        let expires = retention_expiry(signed_at);
        let latest_verify_now = signed_at + MAX_SKEW_PAST_SECS;
        assert!(
            expires > latest_verify_now,
            "a nonce must remain recorded for the whole window a replay could still pass verify"
        );
        assert!(expires - latest_verify_now >= MAX_SKEW_FUTURE_SECS);
    }
}
