use serde::Deserialize;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::events::{
    decode_log, AuthzEvent, ALL_TOPICS, ESTATE_REGISTRY_MAINNET, LAND_REGISTRY_MAINNET,
    LAND_START_BLOCK_MAINNET,
};

pub const CURSOR_ID: &str = "ethereum-mainnet";

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("rpc transport: {0}")]
    Transport(String),
    #[error("rpc returned an error that carries no usable block range: {0}")]
    Rpc(String),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("could not decode a log at block {block} index {index}: {source}")]
    Decode {
        block: i64,
        index: i32,
        #[source]
        source: crate::events::DecodeError,
    },
}

#[derive(Debug, Deserialize)]
struct RpcLog {
    address: String,
    topics: Vec<String>,
    data: String,
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "logIndex")]
    log_index: String,
    #[serde(rename = "blockTimestamp")]
    block_timestamp: Option<String>,
}

fn hex_to_i64(s: &str) -> Option<i64> {
    i64::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// The public RPC caps a filtered `eth_getLogs` at ten thousand rows but names
/// a range that would fit; following that hint is what keeps a full history
/// scan to tens of requests instead of thousands of fixed-width windows.
fn suggested_upper_bound(message: &str) -> Option<u64> {
    let open = message.rfind('[')?;
    let close = message[open..].find(']')? + open;
    let inner = &message[open + 1..close];
    let (_, hi) = inner.split_once(',')?;
    u64::from_str_radix(hi.trim().trim_start_matches("0x"), 16).ok()
}

pub struct Indexer {
    http: reqwest::Client,
    rpc_url: String,
    land_registry: String,
    estate_registry: String,
}

impl Indexer {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            rpc_url: rpc_url.into(),
            land_registry: LAND_REGISTRY_MAINNET.to_string(),
            estate_registry: ESTATE_REGISTRY_MAINNET.to_string(),
        }
    }

    pub fn with_registries(mut self, land: impl Into<String>, estate: impl Into<String>) -> Self {
        self.land_registry = land.into().to_lowercase();
        self.estate_registry = estate.into().to_lowercase();
        self
    }

    async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, IndexError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": method, "params": params
        });
        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| IndexError::Transport(e.to_string()))?;
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| IndexError::Transport(e.to_string()))?;
        Ok(value)
    }

    pub async fn head_block(&self) -> Result<u64, IndexError> {
        let v = self.call("eth_blockNumber", serde_json::json!([])).await?;
        v.get("result")
            .and_then(|r| r.as_str())
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .ok_or_else(|| IndexError::Rpc(format!("eth_blockNumber returned {v}")))
    }

    async fn get_logs(&self, from: u64, to: u64) -> Result<(Vec<RpcLog>, u64), IndexError> {
        let mut upper = to;
        loop {
            let params = serde_json::json!([{
                "fromBlock": format!("0x{from:x}"),
                "toBlock": format!("0x{upper:x}"),
                "address": [self.land_registry, self.estate_registry],
                "topics": [ALL_TOPICS],
            }]);
            let v = self.call("eth_getLogs", params).await?;
            if let Some(result) = v.get("result") {
                let logs: Vec<RpcLog> = serde_json::from_value(result.clone())
                    .map_err(|e| IndexError::Rpc(format!("cannot read logs: {e}")))?;
                return Ok((logs, upper));
            }
            let message = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("no result and no error")
                .to_string();
            match suggested_upper_bound(&message) {
                Some(hi) if hi > from && hi < upper => upper = hi,
                _ => {
                    let halved = from + (upper - from) / 2;
                    if halved <= from {
                        return Err(IndexError::Rpc(message));
                    }
                    upper = halved;
                }
            }
        }
    }

    pub async fn sync(&self, pool: &PgPool, from: u64, to: u64) -> Result<usize, IndexError> {
        let mut cursor = from;
        let mut total = 0usize;
        while cursor <= to {
            let (logs, reached) = self.get_logs(cursor, to).await?;
            let mut events = Vec::with_capacity(logs.len());
            for log in &logs {
                let block = hex_to_i64(&log.block_number).ok_or_else(|| {
                    IndexError::Rpc(format!("bad blockNumber {}", log.block_number))
                })?;
                let index = hex_to_i64(&log.log_index)
                    .ok_or_else(|| IndexError::Rpc(format!("bad logIndex {}", log.log_index)))?
                    as i32;
                let time = log
                    .block_timestamp
                    .as_deref()
                    .and_then(hex_to_i64)
                    .unwrap_or(0);
                match decode_log(&log.address, &log.topics, &log.data, block, index, time) {
                    Ok(ev) => events.push(ev),
                    Err(crate::events::DecodeError::UnhandledTopic(t)) => {
                        warn!(topic = %t, "ignoring a log this indexer does not model")
                    }
                    Err(source) => {
                        return Err(IndexError::Decode {
                            block,
                            index,
                            source,
                        })
                    }
                }
            }
            total += insert_events(pool, &events).await?;
            set_cursor(pool, reached as i64).await?;
            info!(
                from = cursor,
                to = reached,
                events = events.len(),
                "indexed range"
            );
            cursor = reached + 1;
        }
        Ok(total)
    }
}

pub async fn insert_events(pool: &PgPool, events: &[AuthzEvent]) -> Result<usize, sqlx::Error> {
    insert_events_in(pool, "land_authz", events).await
}

pub async fn insert_events_in(
    pool: &PgPool,
    schema: &str,
    events: &[AuthzEvent],
) -> Result<usize, sqlx::Error> {
    if events.is_empty() {
        return Ok(0);
    }
    let mut written = 0usize;
    for chunk in events.chunks(1000) {
        let mut blocks = Vec::with_capacity(chunk.len());
        let mut indexes = Vec::with_capacity(chunk.len());
        let mut times = Vec::with_capacity(chunk.len());
        let mut addrs = Vec::with_capacity(chunk.len());
        let mut kinds = Vec::with_capacity(chunk.len());
        let mut token_ids: Vec<Option<String>> = Vec::with_capacity(chunk.len());
        let mut accounts = Vec::with_capacity(chunk.len());
        let mut operators = Vec::with_capacity(chunk.len());
        let mut approvals = Vec::with_capacity(chunk.len());
        for e in chunk {
            blocks.push(e.block_number);
            indexes.push(e.log_index);
            times.push(e.block_time);
            addrs.push(e.token_address.clone());
            kinds.push(e.kind.to_string());
            token_ids.push(e.token_id.clone());
            accounts.push(e.account.clone());
            operators.push(e.operator.clone());
            approvals.push(e.approved);
        }
        let sql = format!(
            "INSERT INTO {schema}.authz_event \
             (block_number, log_index, block_time, token_address, kind, token_id, account, operator, approved) \
             SELECT b, i, t, a, k, tid::numeric, acct, op, appr \
             FROM UNNEST($1::bigint[], $2::int[], $3::bigint[], $4::text[], $5::text[], \
                         $6::text[], $7::text[], $8::text[], $9::boolean[]) \
                  AS u(b, i, t, a, k, tid, acct, op, appr) \
             ON CONFLICT (block_number, log_index) DO NOTHING",
        );
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&blocks)
            .bind(&indexes)
            .bind(&times)
            .bind(&addrs)
            .bind(&kinds)
            .bind(&token_ids)
            .bind(&accounts)
            .bind(&operators)
            .bind(&approvals)
            .execute(pool)
            .await?;
        written += result.rows_affected() as usize;
    }
    Ok(written)
}

pub async fn set_cursor(pool: &PgPool, block: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO land_authz.index_cursor (id, last_block, updated_at) \
         VALUES ($1, $2, now()) \
         ON CONFLICT (id) DO UPDATE SET last_block = EXCLUDED.last_block, updated_at = now()",
    )
    .bind(CURSOR_ID)
    .bind(block)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn cursor(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let last: Option<i64> =
        sqlx::query_scalar("SELECT last_block FROM land_authz.index_cursor WHERE id = $1")
            .bind(CURSOR_ID)
            .fetch_optional(pool)
            .await?;
    Ok(last
        .map(|b| b as u64 + 1)
        .unwrap_or(LAND_START_BLOCK_MAINNET))
}

/// Folds the event log into current rights. Both per-token legs are decided by
/// the single latest event that touches them, so "granted then revoked" and
/// "granted then transferred away" both end denied without replaying state in
/// application code. The zero address is stored as no grant at all.
pub async fn fold(pool: &PgPool) -> Result<(u64, u64), sqlx::Error> {
    fold_in(pool, "land_authz").await
}

pub async fn fold_in(pool: &PgPool, schema: &str) -> Result<(u64, u64), sqlx::Error> {
    let token_rows = sqlx::query(sqlx::AssertSqlSafe(FOLD_TOKEN_SQL.replace("{s}", schema)))
        .execute(pool)
        .await?
        .rows_affected();
    let account_rows = sqlx::query(sqlx::AssertSqlSafe(FOLD_ACCOUNT_SQL.replace("{s}", schema)))
        .execute(pool)
        .await?
        .rows_affected();
    Ok((token_rows, account_rows))
}

const FOLD_TOKEN_SQL: &str = "WITH latest_operator AS (
             SELECT DISTINCT ON (token_address, token_id)
                    token_address, token_id, block_number, log_index,
                    CASE WHEN kind = 'approval' THEN operator END AS operator
             FROM {s}.authz_event
             WHERE token_id IS NOT NULL AND kind IN ('approval', 'transfer')
             ORDER BY token_address, token_id, block_number DESC, log_index DESC
         ),
         latest_update_operator AS (
             SELECT DISTINCT ON (token_address, token_id)
                    token_address, token_id, block_number, log_index,
                    CASE WHEN kind = 'update_operator' THEN operator END AS update_operator
             FROM {s}.authz_event
             WHERE token_id IS NOT NULL AND kind IN ('update_operator', 'transfer')
             ORDER BY token_address, token_id, block_number DESC, log_index DESC
         ),
         merged AS (
             SELECT COALESCE(o.token_address, u.token_address) AS token_address,
                    COALESCE(o.token_id, u.token_id)           AS token_id,
                    NULLIF(o.operator, '0x0000000000000000000000000000000000000000')        AS operator,
                    NULLIF(u.update_operator, '0x0000000000000000000000000000000000000000') AS update_operator,
                    GREATEST(COALESCE(o.block_number, 0), COALESCE(u.block_number, 0))      AS updated_block,
                    GREATEST(COALESCE(o.log_index, 0), COALESCE(u.log_index, 0))            AS updated_log
             FROM latest_operator o
             FULL OUTER JOIN latest_update_operator u
               ON o.token_address = u.token_address AND o.token_id = u.token_id
         )
         INSERT INTO {s}.token_right
             (token_address, token_id, x, y, operator, update_operator, updated_block, updated_log)
         SELECT token_address, token_id,
                {s}.token_x(token_id), {s}.token_y(token_id),
                operator, update_operator, updated_block, updated_log
         FROM merged
         ON CONFLICT (token_address, token_id) DO UPDATE SET
             operator        = EXCLUDED.operator,
             update_operator = EXCLUDED.update_operator,
             x               = EXCLUDED.x,
             y               = EXCLUDED.y,
             updated_block   = EXCLUDED.updated_block,
             updated_log     = EXCLUDED.updated_log";

const FOLD_ACCOUNT_SQL: &str = "WITH latest AS (
             SELECT DISTINCT ON (token_address, account, operator, kind)
                    token_address, account, operator, kind, approved, block_number, log_index
             FROM {s}.authz_event
             WHERE kind IN ('update_manager', 'approved_for_all')
               AND account IS NOT NULL AND operator IS NOT NULL
             ORDER BY token_address, account, operator, kind, block_number DESC, log_index DESC
         )
         INSERT INTO {s}.account_right
             (token_address, account, operator, kind, is_approved, updated_block, updated_log)
         SELECT token_address, account, operator, kind, COALESCE(approved, false), block_number, log_index
         FROM latest
         ON CONFLICT (token_address, account, operator, kind) DO UPDATE SET
             is_approved   = EXCLUDED.is_approved,
             updated_block = EXCLUDED.updated_block,
             updated_log   = EXCLUDED.updated_log";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_upper_bound_the_rpc_suggests() {
        let msg =
            "Log response size exceeded. ... this block range should work: [0x4b7fc2, 0xe95489]";
        assert_eq!(suggested_upper_bound(msg), Some(0xe95489));
    }

    #[test]
    fn a_message_without_a_range_yields_nothing() {
        assert_eq!(suggested_upper_bound("query timeout"), None);
    }

    #[test]
    fn hex_block_numbers_parse() {
        assert_eq!(hex_to_i64("0x589fce"), Some(5_808_078));
        assert_eq!(hex_to_i64("0x0"), Some(0));
    }
}
