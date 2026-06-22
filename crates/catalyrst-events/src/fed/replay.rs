//! Replay protection for events signed actions (00-primitives.md §2.2).
//!
//! Mirrors the places fed replay: a per-(signer,nonce) row in `seen_nonces`
//! with an `expires_at = signed_at + MAX_SKEW_PAST_SECS` watermark. The insert
//! IS the check — `ON CONFLICT DO NOTHING` + `rows_affected()==0` means the
//! nonce was already seen (replay). DB-backed only; the table is shared with the
//! places crate over the same places_events pool.

use catalyrst_fed::sig::MAX_SKEW_PAST_SECS;
use catalyrst_fed::FedError;
use sqlx::PgPool;

/// Returns Ok(()) if the nonce is fresh (and records it), Err(DuplicateNonce)
/// if already seen.
pub async fn check_and_record(
    pool: &PgPool,
    signer: &str,
    nonce: &[u8; 16],
    signed_at: i64,
) -> Result<(), FedError> {
    let signer = signer.to_ascii_lowercase();
    let nonce_hex = hex::encode(nonce);
    let expires_at = signed_at + MAX_SKEW_PAST_SECS;

    let now = chrono::Utc::now().timestamp();
    let _ = sqlx::query("DELETE FROM seen_nonces WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await;

    let res = sqlx::query(
        "INSERT INTO seen_nonces (signer, nonce, expires_at) VALUES ($1,$2,$3) \
         ON CONFLICT (signer, nonce) DO NOTHING",
    )
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
