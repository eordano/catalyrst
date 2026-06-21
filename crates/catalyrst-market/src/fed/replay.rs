use std::collections::HashMap;
use std::sync::Arc;

use catalyrst_fed::sig::MAX_SKEW_PAST_SECS;
use catalyrst_fed::FedError;
use parking_lot::Mutex;
use sqlx::PgPool;

const LRU_CAP_PER_SIGNER: usize = 65_536;

struct PerSigner {
    order: std::collections::VecDeque<String>,
    set: std::collections::HashSet<String>,
}

impl PerSigner {
    fn new() -> Self {
        Self {
            order: std::collections::VecDeque::with_capacity(64),
            set: std::collections::HashSet::with_capacity(64),
        }
    }

    fn contains(&self, nonce: &str) -> bool {
        self.set.contains(nonce)
    }

    fn insert(&mut self, nonce: String) {
        if self.set.insert(nonce.clone()) {
            self.order.push_back(nonce);
            while self.order.len() > LRU_CAP_PER_SIGNER {
                if let Some(old) = self.order.pop_front() {
                    self.set.remove(&old);
                }
            }
        }
    }
}

pub struct Replay {
    pool: PgPool,
    by_signer: Mutex<HashMap<String, PerSigner>>,
}

impl Replay {
    pub async fn new(pool: PgPool) -> Result<Arc<Self>, sqlx::Error> {
        let me = Arc::new(Self {
            pool: pool.clone(),
            by_signer: Mutex::new(HashMap::new()),
        });
        let now = chrono::Utc::now().timestamp();
        sqlx::query("DELETE FROM market_seen_nonces WHERE expires_at < $1")
            .bind(now)
            .execute(&pool)
            .await?;
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT signer, nonce FROM market_seen_nonces WHERE expires_at >= $1")
                .bind(now)
                .fetch_all(&pool)
                .await?;
        let mut map = me.by_signer.lock();
        for (signer, nonce) in rows {
            map.entry(signer.to_ascii_lowercase())
                .or_insert_with(PerSigner::new)
                .insert(nonce);
        }
        drop(map);
        Ok(me)
    }

    pub async fn check_and_record(
        &self,
        signer: &str,
        nonce: &[u8; 16],
        signed_at: i64,
    ) -> Result<(), FedError> {
        let signer_key = signer.to_ascii_lowercase();
        let nonce_hex = hex::encode(nonce);

        {
            let map = self.by_signer.lock();
            if let Some(ps) = map.get(&signer_key) {
                if ps.contains(&nonce_hex) {
                    return Err(FedError::DuplicateNonce { signer: signer_key });
                }
            }
        }

        let exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM market_seen_nonces WHERE signer = $1 AND nonce = $2")
                .bind(&signer_key)
                .bind(&nonce_hex)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| FedError::Transport(e.to_string()))?;
        if exists.is_some() {
            return Err(FedError::DuplicateNonce { signer: signer_key });
        }

        let expires_at = signed_at + MAX_SKEW_PAST_SECS;
        // The INSERT ... ON CONFLICT DO NOTHING is the AUTHORITATIVE gate (the
        // SELECT above is only a fast-path): two concurrent requests can both pass
        // the SELECT, so rely on rows_affected()==0 (conflict ⇒ already seen) to
        // reject the duplicate. Mirrors the communities/places replay fix.
        let res = sqlx::query(
            "INSERT INTO market_seen_nonces (signer, nonce, expires_at) VALUES ($1, $2, $3) \
             ON CONFLICT (signer, nonce) DO NOTHING",
        )
        .bind(&signer_key)
        .bind(&nonce_hex)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| FedError::Transport(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(FedError::DuplicateNonce { signer: signer_key });
        }

        let mut map = self.by_signer.lock();
        map.entry(signer_key)
            .or_insert_with(PerSigner::new)
            .insert(nonce_hex);

        Ok(())
    }
}
