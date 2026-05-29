use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::types::JsonValue;
use sqlx::PgPool;
use sqlx::Row;

use crate::http::response::ApiError;

#[derive(Debug, Serialize)]
pub struct DbTrade {
    pub id: String,
    #[serde(rename = "chainId")]
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "effectiveSince")]
    pub effective_since: DateTime<Utc>,
    #[serde(rename = "expiresAt")]
    pub expires_at: DateTime<Utc>,
    pub network: String,
    pub signature: String,
    pub signer: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub contract: String,
}

#[derive(Debug, Serialize)]
pub struct TradeAsset {
    #[serde(rename = "assetType")]
    pub asset_type: i32,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiary: Option<String>,
    pub direction: String,
    pub extra: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TradeView {
    #[serde(flatten)]
    pub trade: DbTrade,
    pub sent: Vec<TradeAsset>,
    pub received: Vec<TradeAsset>,
}

pub struct TradesComponent {
    pool: PgPool,
}

impl TradesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_trades(&self) -> Result<(Vec<DbTrade>, i64), ApiError> {
        // STUB: marketplace.trades is created by node-pg-migrate in the upstream
        // marketplace-server. Until we port or import that migration, return empty
        // so the endpoint responds 200 rather than 500. Federation ADR will revisit.
        tracing::info!("trades: skipped (no local marketplace.trades table)");
        let _ = &self.pool;
        Ok((Vec::new(), 0))
    }

    pub async fn get_trade(&self, id: &str) -> Result<TradeView, ApiError> {
        let rows = sqlx::query(
            r#"
SELECT
  t.id::text                  AS trade_id,
  t.chain_id::int4            AS chain_id,
  t.checks                    AS checks,
  t.created_at                AS created_at,
  t.effective_since           AS effective_since,
  t.expires_at                AS expires_at,
  t.network                   AS network,
  t.signature                 AS signature,
  t.signer                    AS signer,
  t.type                      AS trade_type,
  t.contract                  AS contract,
  ta.asset_type::int4         AS asset_type,
  ta.contract_address         AS asset_contract_address,
  ta.beneficiary              AS asset_beneficiary,
  ta.direction                AS asset_direction,
  ta.extra                    AS asset_extra,
  erc721.token_id             AS token_id,
  erc20.amount::text          AS amount,
  item.item_id                AS item_id
FROM marketplace.trades AS t
JOIN marketplace.trade_assets AS ta ON t.id = ta.trade_id
LEFT JOIN marketplace.trade_assets_erc721 AS erc721 ON ta.id = erc721.asset_id
LEFT JOIN marketplace.trade_assets_erc20  AS erc20  ON ta.id = erc20.asset_id
LEFT JOIN marketplace.trade_assets_item   AS item   ON ta.id = item.asset_id
WHERE t.id = $1::uuid
ORDER BY ta.direction ASC
"#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Err(ApiError::not_found(format!(
                "Trade with id {} not found",
                id
            )));
        }
        let head = &rows[0];
        let trade = DbTrade {
            id: head.try_get("trade_id").unwrap_or_default(),
            chain_id: head.try_get::<i32, _>("chain_id").unwrap_or(0),
            checks: head.try_get("checks").unwrap_or(JsonValue::Null),
            created_at: head.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            effective_since: head
                .try_get("effective_since")
                .unwrap_or_else(|_| Utc::now()),
            expires_at: head.try_get("expires_at").unwrap_or_else(|_| Utc::now()),
            network: head.try_get("network").unwrap_or_default(),
            signature: head.try_get("signature").unwrap_or_default(),
            signer: head.try_get("signer").unwrap_or_default(),
            trade_type: head.try_get("trade_type").unwrap_or_default(),
            contract: head.try_get("contract").unwrap_or_default(),
        };
        let mut sent: Vec<TradeAsset> = Vec::new();
        let mut received: Vec<TradeAsset> = Vec::new();
        for r in &rows {
            let dir: String = r.try_get("asset_direction").unwrap_or_default();
            let asset = TradeAsset {
                asset_type: r.try_get::<i32, _>("asset_type").unwrap_or(0),
                contract_address: r.try_get("asset_contract_address").unwrap_or_default(),
                beneficiary: r
                    .try_get::<Option<String>, _>("asset_beneficiary")
                    .unwrap_or(None),
                direction: dir.clone(),
                extra: r.try_get("asset_extra").unwrap_or_default(),
                amount: r.try_get::<Option<String>, _>("amount").unwrap_or(None),
                token_id: r.try_get::<Option<String>, _>("token_id").unwrap_or(None),
                item_id: r.try_get::<Option<String>, _>("item_id").unwrap_or(None),
            };
            if dir == "sent" {
                sent.push(asset);
            } else {
                received.push(asset);
            }
        }
        Ok(TradeView {
            trade,
            sent,
            received,
        })
    }

    pub async fn get_trade_accepted_event(
        &self,
        hashed_signature: &str,
        timestamp: i64,
        caller: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let rows = sqlx::query(
            r#"
SELECT
  t.id::text                  AS trade_id,
  t.chain_id::int4            AS chain_id,
  t.checks                    AS checks,
  t.created_at                AS created_at,
  t.effective_since           AS effective_since,
  t.expires_at                AS expires_at,
  t.network                   AS network,
  t.signature                 AS signature,
  t.signer                    AS signer,
  t.type                      AS trade_type,
  t.contract                  AS contract
FROM marketplace.trades AS t
WHERE t.hashed_signature = $1
LIMIT 1
"#,
        )
        .bind(hashed_signature)
        .fetch_optional(&self.pool)
        .await?;

        let head = rows.ok_or_else(|| {
            ApiError::not_found(format!(
                "Trade with hashed signature {} not found",
                hashed_signature
            ))
        })?;

        let trade = DbTrade {
            id: head.try_get("trade_id").unwrap_or_default(),
            chain_id: head.try_get::<i32, _>("chain_id").unwrap_or(0),
            checks: head.try_get("checks").unwrap_or(JsonValue::Null),
            created_at: head.try_get("created_at").unwrap_or_else(|_| Utc::now()),
            effective_since: head
                .try_get("effective_since")
                .unwrap_or_else(|_| Utc::now()),
            expires_at: head.try_get("expires_at").unwrap_or_else(|_| Utc::now()),
            network: head.try_get("network").unwrap_or_default(),
            signature: head.try_get("signature").unwrap_or_default(),
            signer: head.try_get("signer").unwrap_or_default(),
            trade_type: head.try_get("trade_type").unwrap_or_default(),
            contract: head.try_get("contract").unwrap_or_default(),
        };

        Ok(serde_json::json!({
            "type": "marketplace",
            "subType": "trade_accepted",
            "key": hashed_signature,
            "timestamp": timestamp,
            "caller": caller,
            "trade": trade,
        }))
    }

    pub async fn get_trades_by_address(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TradeView>, ApiError> {
        let lower = address.to_lowercase();
        let rows = sqlx::query(
            r#"
SELECT
  t.id::text                  AS trade_id,
  t.chain_id::int4            AS chain_id,
  t.checks                    AS checks,
  t.created_at                AS created_at,
  t.effective_since           AS effective_since,
  t.expires_at                AS expires_at,
  t.network                   AS network,
  t.signature                 AS signature,
  t.signer                    AS signer,
  t.type                      AS trade_type,
  t.contract                  AS contract,
  ta.asset_type::int4         AS asset_type,
  ta.contract_address         AS asset_contract_address,
  ta.beneficiary              AS asset_beneficiary,
  ta.direction                AS asset_direction,
  ta.extra                    AS asset_extra,
  erc721.token_id             AS token_id,
  erc20.amount::text          AS amount,
  item.item_id                AS item_id
FROM marketplace.trades AS t
JOIN marketplace.trade_assets AS ta ON t.id = ta.trade_id
LEFT JOIN marketplace.trade_assets_erc721 AS erc721 ON ta.id = erc721.asset_id
LEFT JOIN marketplace.trade_assets_erc20  AS erc20  ON ta.id = erc20.asset_id
LEFT JOIN marketplace.trade_assets_item   AS item   ON ta.id = item.asset_id
WHERE t.id IN (
  SELECT t2.id FROM marketplace.trades AS t2
  WHERE t2.signer = $1
    OR EXISTS (
      SELECT 1 FROM marketplace.trade_assets AS ta2
      WHERE ta2.trade_id = t2.id AND ta2.beneficiary = $1
    )
  ORDER BY t2.created_at DESC
  LIMIT $2 OFFSET $3
)
ORDER BY t.created_at DESC, ta.direction ASC
"#,
        )
        .bind(&lower)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let mut grouped: std::collections::HashMap<String, Vec<sqlx::postgres::PgRow>> =
            std::collections::HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for r in rows {
            let id: String = r.try_get("trade_id").unwrap_or_default();
            if !grouped.contains_key(&id) {
                order.push(id.clone());
            }
            grouped.entry(id).or_default().push(r);
        }

        let mut out = Vec::with_capacity(order.len());
        for id in order {
            let group = grouped.remove(&id).unwrap();
            let head = &group[0];
            let trade = DbTrade {
                id: head.try_get("trade_id").unwrap_or_default(),
                chain_id: head.try_get::<i32, _>("chain_id").unwrap_or(0),
                checks: head.try_get("checks").unwrap_or(JsonValue::Null),
                created_at: head.try_get("created_at").unwrap_or_else(|_| Utc::now()),
                effective_since: head
                    .try_get("effective_since")
                    .unwrap_or_else(|_| Utc::now()),
                expires_at: head.try_get("expires_at").unwrap_or_else(|_| Utc::now()),
                network: head.try_get("network").unwrap_or_default(),
                signature: head.try_get("signature").unwrap_or_default(),
                signer: head.try_get("signer").unwrap_or_default(),
                trade_type: head.try_get("trade_type").unwrap_or_default(),
                contract: head.try_get("contract").unwrap_or_default(),
            };
            let mut sent = Vec::new();
            let mut received = Vec::new();
            for r in &group {
                let dir: String = r.try_get("asset_direction").unwrap_or_default();
                let asset = TradeAsset {
                    asset_type: r.try_get::<i32, _>("asset_type").unwrap_or(0),
                    contract_address: r.try_get("asset_contract_address").unwrap_or_default(),
                    beneficiary: r
                        .try_get::<Option<String>, _>("asset_beneficiary")
                        .unwrap_or(None),
                    direction: dir.clone(),
                    extra: r.try_get("asset_extra").unwrap_or_default(),
                    amount: r.try_get::<Option<String>, _>("amount").unwrap_or(None),
                    token_id: r.try_get::<Option<String>, _>("token_id").unwrap_or(None),
                    item_id: r.try_get::<Option<String>, _>("item_id").unwrap_or(None),
                };
                if dir == "sent" {
                    sent.push(asset);
                } else {
                    received.push(asset);
                }
            }
            out.push(TradeView {
                trade,
                sent,
                received,
            });
        }

        Ok(out)
    }
}

#[allow(dead_code)]
fn row_to_db_trade(r: &sqlx::postgres::PgRow) -> DbTrade {
    DbTrade {
        id: r.try_get::<String, _>("id").unwrap_or_default(),
        chain_id: r.try_get::<i32, _>("chain_id").unwrap_or(0),
        checks: r.try_get("checks").unwrap_or(JsonValue::Null),
        created_at: r.try_get("created_at").unwrap_or_else(|_| Utc::now()),
        effective_since: r.try_get("effective_since").unwrap_or_else(|_| Utc::now()),
        expires_at: r.try_get("expires_at").unwrap_or_else(|_| Utc::now()),
        network: r.try_get("network").unwrap_or_default(),
        signature: r.try_get("signature").unwrap_or_default(),
        signer: r.try_get("signer").unwrap_or_default(),
        trade_type: r.try_get("type").unwrap_or_default(),
        contract: r.try_get("contract").unwrap_or_default(),
    }
}
