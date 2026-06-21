//! Replay protection for places signed actions (00-primitives.md §2.2).
//!
//! Single-node-correct: a per-(signer,nonce) row in `seen_nonces` with an
//! `expires_at = signed_at + MAX_SKEW_PAST_SECS` watermark. Insert is the
//! check: `ON CONFLICT DO NOTHING` + `rows_affected()==0` means the nonce was
//! already seen (replay). We deliberately keep this DB-backed only (no in-proc
//! LRU like the communities crate) because the places writer pool is the single
//! authority and the table is small; a process restart loses nothing.
//!
//! Bound: a replay arriving after the row is GC'd is still caught by the
//! `signed_at` skew window enforced in `Signed::verify` before we get here.

use catalyrst_fed::sig::MAX_SKEW_PAST_SECS;
use catalyrst_fed::FedError;
use sqlx::PgPool;

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Returns Ok(()) if the nonce is fresh (and records it), Err(DuplicateNonce)
/// if already seen. No-op success when no writer pool is configured.
pub async fn check_and_record(
    pool: Option<&PgPool>,
    signer: &str,
    nonce: &[u8; 16],
    signed_at: i64,
) -> Result<(), FedError> {
    let Some(pool) = pool else {
        return Ok(());
    };
    let signer = signer.to_ascii_lowercase();
    let nonce_hex = hex_encode(nonce);
    let expires_at = signed_at + MAX_SKEW_PAST_SECS;

    // opportunistic GC of expired rows (cheap, indexed).
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
