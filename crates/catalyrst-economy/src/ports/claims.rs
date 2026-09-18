use sqlx::PgPool;

/// One round trip that claims an idempotency key before a broadcast: `claimed` when
/// this call inserted the row, `rearmed` when a prior 'error' row was reset to
/// 'pending', otherwise the pre-existing row's `status`/`tx_hash`. Both `None` with
/// nothing claimed means another claim landed concurrently, read as in flight.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Claim {
    pub claimed: bool,
    pub rearmed: bool,
    pub status: Option<String>,
    pub tx_hash: Option<String>,
}

impl Claim {
    pub fn status(&self) -> &str {
        self.status.as_deref().unwrap_or("pending")
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn claim_escrow_action(
    pool: &PgPool,
    key: &str,
    action: &str,
    collection: &str,
    token_id: &str,
    buyer: Option<&str>,
    escrow_address: &str,
    chain_id: i64,
) -> Result<Claim, sqlx::Error> {
    sqlx::query_as(
        "WITH claim AS (\
           INSERT INTO escrow_actions \
           (idempotency_key, action, collection, token_id, buyer, escrow_address, chain_id, status) \
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending') \
           ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING \
           RETURNING id\
         ), rearm AS (\
           UPDATE escrow_actions SET status = 'pending', updated_at = NOW() \
           WHERE idempotency_key = $1 AND status = 'error' AND NOT EXISTS (SELECT 1 FROM claim) \
           RETURNING id\
         ) \
         SELECT EXISTS (SELECT 1 FROM claim) AS claimed, EXISTS (SELECT 1 FROM rearm) AS rearmed, \
                prior.status, prior.tx_hash \
         FROM (SELECT 1) AS one \
         LEFT JOIN escrow_actions AS prior ON prior.idempotency_key = $1",
    )
    .bind(key)
    .bind(action)
    .bind(collection)
    .bind(token_id)
    .bind(buyer)
    .bind(escrow_address)
    .bind(chain_id)
    .fetch_one(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn claim_name_purchase(
    pool: &PgPool,
    key: &str,
    registrar: &str,
    name: Option<&str>,
    token_id: Option<&str>,
    buyer_address: &str,
    custody_address: &str,
    price_wei: &str,
    chain_id: i64,
    mode: &str,
) -> Result<Claim, sqlx::Error> {
    sqlx::query_as(
        "WITH claim AS (\
           INSERT INTO broker_purchases \
           (idempotency_key, collection, item_id, token_id, buyer_address, escrow_address, price_wei, chain_id, mode, status) \
           VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8, $9, 'pending') \
           ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING \
           RETURNING id\
         ), rearm AS (\
           UPDATE broker_purchases SET status = 'pending', updated_at = NOW() \
           WHERE idempotency_key = $1 AND status = 'error' AND NOT EXISTS (SELECT 1 FROM claim) \
           RETURNING id\
         ) \
         SELECT EXISTS (SELECT 1 FROM claim) AS claimed, EXISTS (SELECT 1 FROM rearm) AS rearmed, \
                prior.status, prior.tx_hash \
         FROM (SELECT 1) AS one \
         LEFT JOIN broker_purchases AS prior ON prior.idempotency_key = $1",
    )
    .bind(key)
    .bind(registrar)
    .bind(name)
    .bind(token_id)
    .bind(buyer_address)
    .bind(custody_address)
    .bind(price_wei)
    .bind(chain_id)
    .bind(mode)
    .fetch_one(pool)
    .await
}

pub async fn claim_name_transfer(
    pool: &PgPool,
    key: &str,
    registrar: &str,
    token_id: &str,
    from_address: &str,
    to_address: &str,
    chain_id: i64,
) -> Result<Claim, sqlx::Error> {
    sqlx::query_as(
        "WITH claim AS (\
           INSERT INTO name_transfers \
           (idempotency_key, registrar, token_id, from_address, to_address, chain_id, status) \
           VALUES ($1, $2, $3, $4, $5, $6, 'pending') \
           ON CONFLICT (idempotency_key) DO NOTHING \
           RETURNING id\
         ), rearm AS (\
           UPDATE name_transfers SET status = 'pending', updated_at = NOW() \
           WHERE idempotency_key = $1 AND status = 'error' AND NOT EXISTS (SELECT 1 FROM claim) \
           RETURNING id\
         ) \
         SELECT EXISTS (SELECT 1 FROM claim) AS claimed, EXISTS (SELECT 1 FROM rearm) AS rearmed, \
                prior.status, prior.tx_hash \
         FROM (SELECT 1) AS one \
         LEFT JOIN name_transfers AS prior ON prior.idempotency_key = $1",
    )
    .bind(key)
    .bind(registrar)
    .bind(token_id)
    .bind(from_address)
    .bind(to_address)
    .bind(chain_id)
    .fetch_one(pool)
    .await
}
