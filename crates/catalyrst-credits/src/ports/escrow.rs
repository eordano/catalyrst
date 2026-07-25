use std::time::Duration;

use sqlx::Row;

use crate::http::ApiError;
use crate::ports::credits::CreditsComponent;

const RELEASE_BATCH_LIMIT: i64 = 50;

impl CreditsComponent {
    pub async fn revoke_usage_grant(
        &self,
        usage_grants_pool: &sqlx::PgPool,
        escrow_ref: &str,
    ) -> Result<u64, ApiError> {
        let res = sqlx::query(
            "UPDATE marketplace.usage_grants \
             SET status = 'revoked' \
             WHERE escrow_ref = $1 AND status = 'active'",
        )
        .bind(escrow_ref)
        .execute(usage_grants_pool)
        .await?;
        Ok(res.rows_affected())
    }
}

pub async fn reclaim_escrowed(
    http: &reqwest::Client,
    economy_base_url: &str,
    economy_admin_token: &str,
    collection: &str,
    token_id: &str,
    idempotency_key: &str,
) -> Result<String, ApiError> {
    let body = serde_json::json!({ "collection": collection, "tokenId": token_id });
    post_escrow(
        http,
        &format!("{economy_base_url}/v1/escrow/reclaim"),
        economy_admin_token,
        idempotency_key,
        &body,
    )
    .await
}

pub async fn release_escrowed(
    http: &reqwest::Client,
    economy_base_url: &str,
    economy_admin_token: &str,
    collection: &str,
    token_id: &str,
    buyer: &str,
    idempotency_key: &str,
) -> Result<String, ApiError> {
    let body = serde_json::json!({ "collection": collection, "tokenId": token_id, "buyer": buyer });
    post_escrow(
        http,
        &format!("{economy_base_url}/v1/escrow/release"),
        economy_admin_token,
        idempotency_key,
        &body,
    )
    .await
}

async fn post_escrow(
    http: &reqwest::Client,
    url: &str,
    token: &str,
    idempotency_key: &str,
    body: &serde_json::Value,
) -> Result<String, ApiError> {
    let resp = http
        .post(url)
        .bearer_auth(token)
        .header("Idempotency-Key", idempotency_key)
        .json(body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("economy request failed: {e}")))?;

    let status = resp.status();
    if status.is_success() {
        let parsed: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("economy parse failed: {e}")))?;
        parsed
            .get("txHash")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| ApiError::Internal("economy 200 missing txHash".into()))
    } else {
        let code = status.as_u16();
        let txt = resp.text().await.unwrap_or_default();
        Err(ApiError::Internal(format!(
            "economy status {code}: {}",
            truncate(&txt, 300)
        )))
    }
}

struct DueGrant {
    id: i64,
    buyer: String,
    urn: String,
    token_id: Option<String>,
    collection: Option<String>,
}

#[derive(Clone)]
pub struct ReleaseWorker {
    pub http: reqwest::Client,
    pub economy_base_url: String,
    pub economy_admin_token: Option<String>,
    pub escrow_address: Option<String>,

    pub usage_grants_pool: Option<sqlx::PgPool>,
}

impl ReleaseWorker {
    pub fn spawn(self, interval_secs: u64) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                if let Err(e) = self.run_once().await {
                    tracing::warn!(error = %e, "escrow release sweep failed");
                }
            }
        });
    }

    pub async fn run_once(&self) -> Result<usize, ApiError> {
        let (Some(pool), Some(token)) = (
            self.usage_grants_pool.as_ref(),
            self.economy_admin_token.as_ref(),
        ) else {
            tracing::warn!(
                "escrow release worker idle: USAGE_GRANTS_PG / CATALYRST_ECONOMY_ADMIN_TOKEN unset"
            );
            return Ok(0);
        };
        if self.escrow_address.is_none() {
            tracing::warn!("escrow release worker idle: LANDILER_ESCROW_ADDRESS unset");
            return Ok(0);
        }

        let due: Vec<DueGrant> = sqlx::query(
            "SELECT id, grantee_address, urn, token_id, collection \
             FROM marketplace.usage_grants \
             WHERE status = 'active' AND unlock_at <= now() \
             ORDER BY id LIMIT $1",
        )
        .bind(RELEASE_BATCH_LIMIT)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| DueGrant {
            id: r.get("id"),
            buyer: r.get("grantee_address"),
            urn: r.get("urn"),
            token_id: r.get("token_id"),
            collection: r.get("collection"),
        })
        .collect();

        if due.is_empty() {
            return Ok(0);
        }

        let mut released = 0usize;
        for g in due {
            if self.release_one(pool, token, &g).await {
                released += 1;
            }
        }
        Ok(released)
    }

    async fn release_one(&self, pool: &sqlx::PgPool, token: &str, g: &DueGrant) -> bool {
        let Some(token_id) = g.token_id.as_deref() else {
            tracing::warn!(
                grant_id = g.id,
                urn = %g.urn,
                "release skipped: usage_grant has no token_id (primary mint); the minted \
                 tokenId must be resolved from the escrow Leased event / buy receipt (follow-up)"
            );
            return false;
        };
        let Some(collection) = g.collection.as_deref() else {
            tracing::warn!(
                grant_id = g.id,
                urn = %g.urn,
                "release skipped: usage_grant has no collection (pre-Phase-6 / manual grant)"
            );
            return false;
        };

        let idem = format!("release:grant:{}", g.id);
        match release_escrowed(
            &self.http,
            &self.economy_base_url,
            token,
            collection,
            token_id,
            &g.buyer,
            &idem,
        )
        .await
        {
            Ok(tx_hash) => {
                let mut db_tx = match pool.begin().await {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::error!(
                            grant_id = g.id,
                            error = %e,
                            "release broadcast OK but opening tx to mark 'released' FAILED; retries next tick"
                        );
                        return false;
                    }
                };

                let locked: Option<String> = match sqlx::query_scalar(
                    "SELECT status FROM marketplace.usage_grants \
                     WHERE id = $1 FOR UPDATE",
                )
                .bind(g.id)
                .fetch_optional(&mut *db_tx)
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = db_tx.rollback().await;
                        tracing::error!(
                            grant_id = g.id,
                            error = %e,
                            "release broadcast OK but locking grant row FAILED; retries next tick"
                        );
                        return false;
                    }
                };

                match locked.as_deref() {
                    Some("active") => {
                        let res = sqlx::query(
                            "UPDATE marketplace.usage_grants \
                             SET status = 'released' \
                             WHERE id = $1 AND status = 'active'",
                        )
                        .bind(g.id)
                        .execute(&mut *db_tx)
                        .await;
                        match res {
                            Ok(_) => match db_tx.commit().await {
                                Ok(()) => {
                                    tracing::info!(
                                        grant_id = g.id,
                                        urn = %g.urn,
                                        tx_hash = %tx_hash,
                                        "escrow grant released to buyer at unlock"
                                    );
                                    true
                                }
                                Err(e) => {
                                    tracing::error!(
                                        grant_id = g.id,
                                        error = %e,
                                        "release broadcast OK but committing 'released' FAILED; retries next tick"
                                    );
                                    false
                                }
                            },
                            Err(e) => {
                                let _ = db_tx.rollback().await;
                                tracing::error!(
                                    grant_id = g.id,
                                    error = %e,
                                    "release broadcast OK but marking grant 'released' FAILED; retries next tick"
                                );
                                false
                            }
                        }
                    }
                    Some("released") => {
                        let _ = db_tx.rollback().await;
                        tracing::info!(
                            grant_id = g.id,
                            urn = %g.urn,
                            "release: grant already 'released'; idempotent no-op"
                        );
                        true
                    }
                    other => {
                        let _ = db_tx.rollback().await;
                        tracing::warn!(
                            grant_id = g.id,
                            urn = %g.urn,
                            status = ?other,
                            "release broadcast OK but grant is no longer 'active'; leaving status untouched"
                        );
                        false
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    grant_id = g.id,
                    urn = %g.urn,
                    error = %e,
                    "escrow release call failed; grant stays 'active' and retries next tick"
                );
                false
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    &s[..end]
}
