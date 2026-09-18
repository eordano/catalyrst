use std::time::Duration;

use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use catalyrst_fed::sig::MAX_SKEW_PAST_SECS;
use catalyrst_fed::FedError;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

/// Same retention as catalyrst_fed's NonceReplayGuard; the expiry sweep runs in
/// `spawn_sweep` instead of ahead of every signed request.
const REPLAY_RETENTION_SLACK_SECS: i64 = 60;
const SWEEP_INTERVAL: Duration = Duration::from_secs(300);
const INSERT_SQL: &str = "INSERT INTO seen_nonces (signer, nonce, expires_at) VALUES ($1,$2,$3) \
     ON CONFLICT (signer, nonce) DO NOTHING";
const SWEEP_SQL: &str = "DELETE FROM seen_nonces WHERE expires_at < $1";

pub fn spawn_sweep(pool: PgPool) {
    spawn_periodic(
        "events-nonce-sweep",
        SWEEP_INTERVAL,
        PeriodicCfg::default(),
        CancellationToken::new(),
        move || {
            let pool = pool.clone();
            async move {
                sqlx::query(SWEEP_SQL)
                    .bind(chrono::Utc::now().timestamp())
                    .execute(&pool)
                    .await
                    .map(|_| ())
            }
        },
    );
}

pub async fn check_and_record(
    pool: &PgPool,
    signer: &str,
    nonce: &[u8; 16],
    signed_at: i64,
) -> Result<(), FedError> {
    let signer = signer.to_ascii_lowercase();
    let res = sqlx::query(INSERT_SQL)
        .bind(&signer)
        .bind(hex::encode(nonce))
        .bind(signed_at + MAX_SKEW_PAST_SECS + REPLAY_RETENTION_SLACK_SECS)
        .execute(pool)
        .await
        .map_err(|e| FedError::Transport(e.to_string()))?;
    if res.rows_affected() == 0 {
        return Err(FedError::DuplicateNonce { signer });
    }
    Ok(())
}
