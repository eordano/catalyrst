use chrono::Utc;
use sqlx::types::JsonValue;
use sqlx::PgPool;
use sqlx::Row;

use crate::http::response::ApiError;

use super::events::{bid_accepted_event, item_sold_event, AssetMeta};
use super::types::{DbTrade, DbTradeListRow, PublicTradeAsset, Trade, TradeAsset, TradeView};
use super::{ASSET_TYPE_COLLECTION_ITEM, ASSET_TYPE_ERC721};

pub struct TradesComponent {
    pool: PgPool,

    paginate: bool,
}

fn is_missing_trades_table(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e {
        match db.code().as_deref() {
            Some("42P01") => {
                return db.message().contains("marketplace.trades")
                    || db.message().contains("trade_assets")
                    || db.message().contains("trade_type")
            }
            Some("42501") | Some("3F000") => return db.message().contains("marketplace"),
            _ => {}
        }
    }
    false
}

impl TradesComponent {
    pub fn new(pool: PgPool, paginate: bool) -> Self {
        Self { pool, paginate }
    }

    pub async fn list_trades(
        &self,
        first: Option<i64>,
        skip: Option<i64>,
    ) -> Result<(Vec<DbTradeListRow>, i64), ApiError> {
        if !self.paginate {
            return self.get_trades().await;
        }
        let limit = first.unwrap_or(100).clamp(0, 1000);
        let offset = skip.unwrap_or(0).max(0);
        let rows = sqlx::query(
            r#"
SELECT id::text AS id, chain_id::int4 AS chain_id, checks, created_at,
       effective_since, expires_at, network, signature, signer,
       type::text AS type, contract
FROM marketplace.trades
ORDER BY created_at DESC
LIMIT $1 OFFSET $2
"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) {
                Ok(Vec::new())
            } else {
                Err(e)
            }
        })?;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM marketplace.trades")
            .fetch_one(&self.pool)
            .await
            .or_else(|e| {
                if is_missing_trades_table(&e) {
                    Ok(0)
                } else {
                    Err(e)
                }
            })?;
        let data = rows.iter().map(row_to_db_trade_list_row).collect();
        Ok((data, total))
    }

    pub async fn get_trades(&self) -> Result<(Vec<DbTradeListRow>, i64), ApiError> {
        let rows = sqlx::query(
            r#"
SELECT id::text AS id, chain_id::int4 AS chain_id, checks, created_at,
       effective_since, expires_at, network, signature, signer,
       type::text AS type, contract
FROM marketplace.trades
"#,
        )
        .fetch_all(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) {
                Ok(Vec::new())
            } else {
                Err(e)
            }
        })?;

        let count = rows.len() as i64;
        let data = rows.iter().map(row_to_db_trade_list_row).collect();
        Ok((data, count))
    }

    pub async fn get_trade(&self, id: &str) -> Result<Trade, ApiError> {
        let view = self.get_trade_view(id).await?;
        let mut trade = Trade::from_view(&view);
        trade.status = self.mv_status_for_trade(id).await?;
        Ok(trade)
    }

    async fn mv_status_for_trade(&self, id: &str) -> Result<Option<String>, ApiError> {
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM marketplace.mv_trades WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) || is_missing_squid(&e) {
                Ok(None)
            } else {
                Err(e)
            }
        })?;
        Ok(status)
    }

    async fn get_trade_view(&self, id: &str) -> Result<TradeView, ApiError> {
        let head_row = sqlx::query(
            r#"
SELECT id::text AS trade_id, chain_id::int4 AS chain_id, checks, created_at,
       effective_since, expires_at, network, signature, signer,
       type::text AS trade_type, contract
FROM marketplace.trades
WHERE id = $1::uuid
"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) {
                Ok(None)
            } else {
                Err(e)
            }
        })?;

        let head = head_row
            .ok_or_else(|| ApiError::not_found(format!("Trade with id {} not found", id)))?;
        let trade = head_to_db_trade(&head);
        let (sent, received) = self.assets_for_trade(&trade.id).await?;
        Ok(TradeView {
            trade,
            sent,
            received,
        })
    }

    async fn assets_for_trade(
        &self,
        trade_id: &str,
    ) -> Result<(Vec<TradeAsset>, Vec<TradeAsset>), ApiError> {
        let asset_rows = sqlx::query(
            r#"
SELECT ta.asset_type::int4 AS asset_type, ta.contract_address AS asset_contract_address,
       ta.beneficiary AS asset_beneficiary, ta.direction::text AS asset_direction,
       ta.extra AS asset_extra,
       erc721.token_id AS token_id, erc20.amount::text AS amount, item.item_id AS item_id
FROM marketplace.trade_assets AS ta
LEFT JOIN marketplace.trade_assets_erc721 AS erc721 ON ta.id = erc721.asset_id
LEFT JOIN marketplace.trade_assets_erc20  AS erc20  ON ta.id = erc20.asset_id
LEFT JOIN marketplace.trade_assets_item   AS item   ON ta.id = item.asset_id
WHERE ta.trade_id = $1::uuid
ORDER BY ta.direction ASC
"#,
        )
        .bind(trade_id)
        .fetch_all(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) {
                Ok(Vec::new())
            } else {
                Err(e)
            }
        })?;

        let mut sent: Vec<TradeAsset> = Vec::new();
        let mut received: Vec<TradeAsset> = Vec::new();
        for r in &asset_rows {
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
        Ok((sent, received))
    }

    pub async fn get_trade_accepted_event(
        &self,
        hashed_signature: &str,
        timestamp: i64,
        caller: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let head_row = sqlx::query(
            r#"
SELECT id::text AS trade_id, chain_id::int4 AS chain_id, checks, created_at,
       effective_since, expires_at, network, signature, signer,
       type::text AS trade_type, contract
FROM marketplace.trades
WHERE hashed_signature = $1
LIMIT 1
"#,
        )
        .bind(hashed_signature)
        .fetch_optional(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) {
                Ok(None)
            } else {
                Err(e)
            }
        })?;

        let head = head_row.ok_or_else(|| {
            ApiError::not_found(format!(
                "Trade with hashed signature {} not found",
                hashed_signature
            ))
        })?;
        let trade_db = head_to_db_trade(&head);
        let (sent, received) = self.assets_for_trade(&trade_db.id).await?;
        let trade = Trade::from_view(&TradeView {
            trade: trade_db,
            sent,
            received,
        });

        let mut event = self
            .get_notification_event_for_trade(&trade, caller)
            .await?
            .ok_or_else(|| ApiError::internal("Notification event was not generated"))?;

        if let Some(obj) = event.as_object_mut() {
            obj.insert("timestamp".into(), serde_json::json!(timestamp));
        }
        Ok(event)
    }

    async fn get_notification_event_for_trade(
        &self,
        trade: &Trade,
        caller: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let mut assets: Vec<Option<AssetMeta>> = Vec::new();
        for a in trade.sent.iter().chain(trade.received.iter()) {
            assets.push(self.resolve_asset_meta(a, &trade.network).await?);
        }
        let resolved: Vec<&AssetMeta> = assets.iter().filter_map(|a| a.as_ref()).collect();

        Ok(match trade.trade_type.as_str() {
            "bid" => bid_accepted_event(trade, &resolved),

            "public_item_order" | "public_nft_order" => item_sold_event(trade, &resolved, caller),
            _ => None,
        })
    }

    async fn resolve_asset_meta(
        &self,
        asset: &PublicTradeAsset,
        network: &str,
    ) -> Result<Option<AssetMeta>, ApiError> {
        match asset.asset_type {
            ASSET_TYPE_ERC721 => {
                let Some(token_id) = asset.token_id.as_deref() else {
                    return Ok(None);
                };
                self.resolve_nft_meta(&asset.contract_address, token_id, network)
                    .await
            }
            ASSET_TYPE_COLLECTION_ITEM => {
                let Some(item_id) = asset.item_id.as_deref() else {
                    return Ok(None);
                };
                self.resolve_item_meta(&asset.contract_address, item_id)
                    .await
            }
            _ => Ok(None),
        }
    }

    async fn resolve_nft_meta(
        &self,
        contract_address: &str,
        token_id: &str,
        network: &str,
    ) -> Result<Option<AssetMeta>, ApiError> {
        let networks = crate::ports::nfts::get_db_networks_for(network);
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
SELECT
  account.address AS owner,
  nft.image       AS image,
  nft.category    AS category,
  COALESCE(wearable.rarity, emote.rarity) AS rarity,
  COALESCE(wearable.name, emote.name, land_data."name", ens.subdomain) AS name
FROM {schema}.nft nft
LEFT JOIN {schema}.metadata metadata ON nft.metadata_id = metadata.id
LEFT JOIN {schema}.wearable wearable ON metadata.wearable_id = wearable.id
LEFT JOIN {schema}.emote    emote    ON metadata.emote_id    = emote.id
LEFT JOIN {schema}.parcel   parcel   ON nft.parcel_id = parcel.id
LEFT JOIN {schema}.estate   estate   ON nft.estate_id = estate.id
LEFT JOIN {schema}.data     land_data ON (estate.data_id = land_data.id OR parcel.data_id = land_data.id)
LEFT JOIN {schema}.ens      ens      ON ens.id = nft.ens_id
LEFT JOIN {schema}.account  account  ON nft.owner_id = account.id
WHERE LOWER(nft.contract_address) = LOWER($1)
  AND nft.token_id = $2::numeric
  AND nft.network = ANY($3)
LIMIT 1
"#,
            schema = crate::MARKETPLACE_SQUID_SCHEMA,
        )))
        .bind(contract_address)
        .bind(token_id)
        .bind(&networks)
        .fetch_optional(&self.pool)
        .await
        .or_else(|e| if is_missing_squid(&e) { Ok(None) } else { Err(e) })?;

        Ok(row.map(|r| AssetMeta {
            image: r
                .try_get::<Option<String>, _>("image")
                .unwrap_or(None)
                .unwrap_or_default(),

            seller: r
                .try_get::<Option<String>, _>("owner")
                .unwrap_or(None)
                .unwrap_or_default(),
            category: r
                .try_get::<Option<String>, _>("category")
                .unwrap_or(None)
                .unwrap_or_default(),
            rarity: r.try_get::<Option<String>, _>("rarity").unwrap_or(None),
            name: r.try_get::<Option<String>, _>("name").unwrap_or(None),
            contract_address: contract_address.to_string(),
            token_id: Some(token_id.to_string()),
            item_id: None,
        }))
    }

    async fn resolve_item_meta(
        &self,
        contract_address: &str,
        item_id: &str,
    ) -> Result<Option<AssetMeta>, ApiError> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
SELECT
  item.image     AS image,
  item.creator   AS creator,
  item.rarity    AS rarity,
  COALESCE(wearable.name, emote.name) AS name,
  item.item_type AS item_type
FROM {schema}.item item
LEFT JOIN {schema}.metadata metadata ON item.metadata_id = metadata.id
LEFT JOIN {schema}.wearable wearable ON metadata.wearable_id = wearable.id
LEFT JOIN {schema}.emote    emote    ON metadata.emote_id    = emote.id
WHERE LOWER(item.collection_id) = LOWER($1)
  AND item.blockchain_id = $2::numeric
LIMIT 1
"#,
            schema = crate::MARKETPLACE_SQUID_SCHEMA,
        )))
        .bind(contract_address)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await
        .or_else(|e| {
            if is_missing_squid(&e) {
                Ok(None)
            } else {
                Err(e)
            }
        })?;

        Ok(row.map(|r| {
            let item_type: Option<String> = r.try_get("item_type").unwrap_or(None);
            AssetMeta {
                image: r
                    .try_get::<Option<String>, _>("image")
                    .unwrap_or(None)
                    .unwrap_or_default(),

                seller: r
                    .try_get::<Option<String>, _>("creator")
                    .unwrap_or(None)
                    .unwrap_or_default(),
                category: category_from_item_type(item_type.as_deref()),
                rarity: r.try_get::<Option<String>, _>("rarity").unwrap_or(None),
                name: r.try_get::<Option<String>, _>("name").unwrap_or(None),
                contract_address: contract_address.to_string(),
                token_id: None,
                item_id: Some(item_id.to_string()),
            }
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
  t.type::text                AS trade_type,
  t.contract                  AS contract,
  ta.asset_type::int4         AS asset_type,
  ta.contract_address         AS asset_contract_address,
  ta.beneficiary              AS asset_beneficiary,
  ta.direction::text          AS asset_direction,
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
        .await
        .or_else(|e| {
            if is_missing_trades_table(&e) {
                Ok(Vec::new())
            } else {
                Err(e)
            }
        })?;

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

fn row_to_db_trade_list_row(r: &sqlx::postgres::PgRow) -> DbTradeListRow {
    DbTradeListRow {
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

fn head_to_db_trade(head: &sqlx::postgres::PgRow) -> DbTrade {
    DbTrade {
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
    }
}

fn is_missing_squid(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e {
        match db.code().as_deref() {
            Some("42P01") | Some("42501") | Some("3F000") => return true,
            _ => {}
        }
    }
    false
}

fn category_from_item_type(item_type: Option<&str>) -> String {
    let wearables = crate::ports::items::get_item_types_from_nft_category(
        crate::dcl_schemas::NftCategory::Wearable,
    );
    match item_type {
        Some(t) if wearables.contains(&t) => "wearable".into(),
        _ => "emote".into(),
    }
}
