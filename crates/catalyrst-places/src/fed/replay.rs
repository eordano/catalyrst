use std::sync::LazyLock;

use catalyrst_envcfg::env_bool;
use catalyrst_fed::replay::NonceReplayGuard;
use catalyrst_fed::FedError;
use sqlx::PgPool;

static GUARD: LazyLock<NonceReplayGuard> = LazyLock::new(|| NonceReplayGuard::new("seen_nonces"));

pub async fn check_and_record(
    pool: Option<&PgPool>,
    signer: &str,
    nonce: &[u8; 16],
    signed_at: i64,
) -> Result<(), FedError> {
    let Some(pool) = pool else {
        if env_bool("PLACES_FED_ALLOW_REPLAY_SKIP", false) {
            tracing::warn!(
                signer,
                "replay protection skipped: no writer pool configured and \
                 PLACES_FED_ALLOW_REPLAY_SKIP is set"
            );
            return Ok(());
        }
        return Err(FedError::Transport(
            "replay guard unavailable: no writer pool configured \
             (set PLACES_FED_ALLOW_REPLAY_SKIP=1 to accept unguarded replays)"
                .to_string(),
        ));
    };
    GUARD.check_and_record(pool, signer, nonce, signed_at).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_pool_fails_closed_unless_skip_env_set() {
        std::env::remove_var("PLACES_FED_ALLOW_REPLAY_SKIP");
        let nonce = [7u8; 16];
        let err = check_and_record(None, "0xabc", &nonce, 0)
            .await
            .unwrap_err();
        assert!(matches!(err, FedError::Transport(_)), "{err}");

        std::env::set_var("PLACES_FED_ALLOW_REPLAY_SKIP", "1");
        assert!(check_and_record(None, "0xabc", &nonce, 0).await.is_ok());
        std::env::remove_var("PLACES_FED_ALLOW_REPLAY_SKIP");
    }
}
