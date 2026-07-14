use std::sync::LazyLock;

use catalyrst_fed::replay::NonceReplayGuard;
use catalyrst_fed::FedError;
use sqlx::PgPool;

static GUARD: LazyLock<NonceReplayGuard> = LazyLock::new(|| NonceReplayGuard::new("seen_nonces"));

pub async fn check_and_record(
    pool: &PgPool,
    signer: &str,
    nonce: &[u8; 16],
    signed_at: i64,
) -> Result<(), FedError> {
    GUARD.check_and_record(pool, signer, nonce, signed_at).await
}
