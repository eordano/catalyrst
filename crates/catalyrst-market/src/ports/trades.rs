use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Serialize, Serializer};
use sqlx::types::JsonValue;
use sqlx::PgPool;
use sqlx::Row;

use crate::http::response::ApiError;

fn ms<S: Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_i64(dt.timestamp_millis())
}

/// ISO-8601 / RFC-3339 with fixed millisecond precision and a `Z` suffix —
/// byte-identical to JavaScript `Date.prototype.toJSON()`, which is how
/// upstream marketplace-server's `getTrades()` serializes the `pg`-decoded
/// `Date` timestamp columns (it `SELECT *`s the trades table and returns the
/// rows untouched).
fn iso<S: Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true))
}

/// `GET /v1/trades` list-row wire shape. Mirrors upstream `DBTrade` exactly:
/// raw snake_case keys (the `pg` driver hands back the column names verbatim)
/// and ISO-8601 string timestamps. This intentionally differs from the
/// camelCase `DbTrade` used elsewhere (e.g. the trade-accepted notification
/// payload), so the list endpoint stays byte-faithful to upstream's
/// `{ data: DBTrade[], count }`.
#[derive(Debug, Serialize)]
pub struct DbTradeListRow {
    pub id: String,
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(serialize_with = "iso")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "iso")]
    pub effective_since: DateTime<Utc>,
    #[serde(serialize_with = "iso")]
    pub expires_at: DateTime<Utc>,
    pub network: String,
    pub signature: String,
    pub signer: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub contract: String,
}

#[derive(Debug, Serialize)]
pub struct DbTrade {
    pub id: String,
    #[serde(rename = "chainId")]
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(rename = "createdAt", serialize_with = "ms")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "effectiveSince", serialize_with = "ms")]
    pub effective_since: DateTime<Utc>,
    #[serde(rename = "expiresAt", serialize_with = "ms")]
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
    #[serde(skip_serializing)]
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

// --- public `Trade` wire shape (`@dcl/schemas` Trade) ----------------------
//
// `GET /v1/trades/{id}` and the `/accept` notification embed a PUBLIC `Trade`,
// not the internal `DBTrade`. Upstream builds it via
// `fromDbTradeAndDBTradeAssetWithValueListToTrade` (adapters/trades/trades.ts),
// which:
//   * emits createdAt as an epoch-millisecond NUMBER (`created_at.getTime()`),
//   * OMITS effective_since / expires_at (those are DBTrade-internal),
//   * shapes each asset as the discriminated `TradeAsset` union — exactly one
//     value field per asset_type (ERC20/USD_PEGGED_MANA → amount, ERC721 →
//     tokenId, COLLECTION_ITEM → itemId), with `received` assets additionally
//     carrying `beneficiary` — and drops the internal `direction` column.

const ASSET_TYPE_ERC20: i32 = 1;
const ASSET_TYPE_USD_PEGGED_MANA: i32 = 2;
const ASSET_TYPE_ERC721: i32 = 3;
const ASSET_TYPE_COLLECTION_ITEM: i32 = 4;

/// One `sent`/`received` asset of a public `Trade`. The discriminated union is
/// realized with `skip_serializing_if`: for any given `asset_type` exactly one
/// of amount/tokenId/itemId is `Some`, so only that key is emitted — byte-equal
/// to upstream's `fromDBTradeAssetWithValueToTradeAsset` switch.
#[derive(Debug, Serialize)]
pub struct PublicTradeAsset {
    #[serde(rename = "assetType")]
    pub asset_type: i32,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    pub extra: String,
    #[serde(rename = "amount", skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiary: Option<String>,
}

impl PublicTradeAsset {
    /// Build the union variant for `asset_type`, mirroring upstream's switch.
    /// `beneficiary` is attached only on the `received` direction (upstream's
    /// `fromDBTradeAssetWithValueToTradeAssetWithBeneficiary`).
    fn from_db(asset: &TradeAsset, with_beneficiary: bool) -> Self {
        let (amount, token_id, item_id) = match asset.asset_type {
            ASSET_TYPE_ERC20 | ASSET_TYPE_USD_PEGGED_MANA => (asset.amount.clone(), None, None),
            ASSET_TYPE_ERC721 => (None, asset.token_id.clone(), None),
            ASSET_TYPE_COLLECTION_ITEM => (None, None, asset.item_id.clone()),
            _ => (None, None, None),
        };
        PublicTradeAsset {
            asset_type: asset.asset_type,
            contract_address: asset.contract_address.clone(),
            extra: asset.extra.clone(),
            amount,
            token_id,
            item_id,
            beneficiary: if with_beneficiary {
                asset.beneficiary.clone()
            } else {
                None
            },
        }
    }
}

/// Public `Trade` (`@dcl/schemas`). camelCase keys, `createdAt` as a number, and
/// NO `effectiveSince`/`expiresAt`.
#[derive(Debug, Serialize)]
pub struct Trade {
    pub id: String,
    pub signer: String,
    pub signature: String,
    #[serde(rename = "type")]
    pub trade_type: String,
    pub network: String,
    #[serde(rename = "chainId")]
    pub chain_id: i32,
    pub checks: JsonValue,
    #[serde(rename = "createdAt", serialize_with = "ms")]
    pub created_at: DateTime<Utc>,
    pub sent: Vec<PublicTradeAsset>,
    pub received: Vec<PublicTradeAsset>,
    pub contract: String,
}

impl Trade {
    /// Port of `fromDbTradeAndDBTradeAssetWithValueListToTrade`.
    fn from_view(view: &TradeView) -> Self {
        Trade {
            id: view.trade.id.clone(),
            signer: view.trade.signer.clone(),
            signature: view.trade.signature.clone(),
            trade_type: view.trade.trade_type.clone(),
            network: view.trade.network.clone(),
            chain_id: view.trade.chain_id,
            checks: view.trade.checks.clone(),
            created_at: view.trade.created_at,
            sent: view
                .sent
                .iter()
                .map(|a| PublicTradeAsset::from_db(a, false))
                .collect(),
            received: view
                .received
                .iter()
                .map(|a| PublicTradeAsset::from_db(a, true))
                .collect(),
            contract: view.trade.contract.clone(),
        }
    }
}

pub struct TradesComponent {
    pool: PgPool,
    /// When true, `list_trades` paginates instead of returning the whole table.
    /// Off by default to preserve upstream parity. See `Config::trades_pagination`.
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

    /// `GET /v1/trades` entry point. With the `trades_pagination` flag off
    /// (default) this returns the whole table — byte-for-byte parity with
    /// upstream marketplace-server's `getTrades()`. With the flag on it returns
    /// a `first`/`skip` page ordered by `created_at DESC` (served by
    /// `idx_trades_created_at`) plus the true total `count`, so the endpoint
    /// loads in well under 200ms instead of serializing the full 43MB table.
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

    /// `GET /v1/trades/{id}`. Returns the PUBLIC `Trade` (no effectiveSince /
    /// expiresAt, createdAt as a number, discriminated assets) — upstream's
    /// `getTrade` → `fromDbTradeAndDBTradeAssetWithValueListToTrade`.
    pub async fn get_trade(&self, id: &str) -> Result<Trade, ApiError> {
        let view = self.get_trade_view(id).await?;
        Ok(Trade::from_view(&view))
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

    /// Fetch a trade's assets, split into (sent, received). Shared by the
    /// `/v1/trades/{id}` and `/accept` paths.
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

    /// `GET /v1/trades/{hashedSignature}/accept`. Mirrors upstream's
    /// `getTradeAcceptedEvent` → `getNotificationEventForTrade(.., ACCEPTED, ..)`:
    /// resolves the trade's assets, then builds the `@dcl/schemas` notification
    /// `Event` — a `blockchain` event whose subType is `bid-accepted` (for BID
    /// trades) or `item-sold` (for public_*_order trades), carrying the full
    /// metadata. The returned event's `timestamp` is overridden with the request
    /// timestamp, exactly as upstream's `{ ...event, timestamp }`. A 404 is
    /// raised when no trade matches the hashed signature; a 500 when the trade
    /// type/assets cannot produce an event (upstream `EventNotGeneratedError`).
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
            .ok_or_else(|| {
                // upstream EventNotGeneratedError → 500.
                ApiError::internal("Notification event was not generated")
            })?;

        // Upstream returns `{ ...event, timestamp }` — request timestamp wins.
        if let Some(obj) = event.as_object_mut() {
            obj.insert("timestamp".into(), serde_json::json!(timestamp));
        }
        Ok(event)
    }

    /// Port of `getNotificationEventForTrade(trade, pg, ACCEPTED, caller)`:
    /// resolves every sent+received asset (ERC20/USD-pegged → no asset, ERC721 →
    /// NFT, COLLECTION_ITEM → item), then dispatches by trade type to the
    /// bid-accepted / item-sold notification builders. Returns `None` when the
    /// type/asset combination yields no notification.
    async fn get_notification_event_for_trade(
        &self,
        trade: &Trade,
        caller: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        // Resolve assets in [...sent, ...received] order, matching upstream.
        let mut assets: Vec<Option<AssetMeta>> = Vec::new();
        for a in trade.sent.iter().chain(trade.received.iter()) {
            assets.push(self.resolve_asset_meta(a, &trade.network).await?);
        }
        let resolved: Vec<&AssetMeta> = assets.iter().filter_map(|a| a.as_ref()).collect();

        Ok(match trade.trade_type.as_str() {
            // BID + ACCEPTED → bid-accepted. Requires exactly one resolved asset.
            "bid" => bid_accepted_event(trade, &resolved),
            // public_item_order / public_nft_order + ACCEPTED → item-sold.
            "public_item_order" | "public_nft_order" => item_sold_event(trade, &resolved, caller),
            _ => None,
        })
    }

    /// Resolve the metadata an asset contributes to a notification. ERC20 /
    /// USD-pegged-MANA assets contribute nothing (upstream resolves them to
    /// `undefined`). ERC721 resolves the squid NFT (owner/image/category/rarity/
    /// name); COLLECTION_ITEM resolves the squid item (creator/image/category/
    /// rarity/name). Mirrors `getNftByTokenIdQuery` / `getItemByItemIdQuery`.
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
        let row = sqlx::query(&format!(
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
        ))
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
            // DBNFT has no `creator`; the notification falls back to `owner`.
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
        let row = sqlx::query(&format!(
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
        ))
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
                // DBItem has `creator`; the notification uses it as `seller`.
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

/// Build a `DbTrade` from a `marketplace.trades` head row selected with the
/// `trade_id`/`chain_id`/`trade_type` aliases used across the trade queries.
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

/// `getCategoryFromDBItem`: wearable item types → "wearable", otherwise "emote".
/// The wearable item-type set is sourced from the canonical
/// `get_item_types_from_nft_category` mapping so it stays in lockstep with the
/// `ItemType` enum.
fn category_from_item_type(item_type: Option<&str>) -> String {
    let wearables = crate::ports::items::get_item_types_from_nft_category(
        crate::dcl_schemas::NftCategory::Wearable,
    );
    match item_type {
        Some(t) if wearables.contains(&t) => "wearable".into(),
        _ => "emote".into(),
    }
}

/// Resolved asset metadata an NFT/item contributes to a notification — the
/// subset of upstream's DBNFT/DBItem the bid-accepted / item-sold builders read.
struct AssetMeta {
    image: String,
    /// `'creator' in asset ? asset.creator : asset.owner` — already collapsed:
    /// items carry creator, NFTs carry owner.
    seller: String,
    category: String,
    rarity: Option<String>,
    name: Option<String>,
    contract_address: String,
    token_id: Option<String>,
    item_id: Option<String>,
}

impl AssetMeta {
    /// `${MARKETPLACE_BASE_URL}/contracts/{c}/tokens/{t}` for NFTs, or
    /// `.../items/{i}` for collection items — upstream's `link`.
    fn link(&self) -> String {
        let base = std::env::var("MARKETPLACE_BASE_URL").unwrap_or_default();
        if let Some(token_id) = &self.token_id {
            format!(
                "{}/contracts/{}/tokens/{}",
                base, self.contract_address, token_id
            )
        } else {
            format!(
                "{}/contracts/{}/items/{}",
                base,
                self.contract_address,
                self.item_id.as_deref().unwrap_or_default()
            )
        }
    }

    /// item-sold's `tokenId` metadata: token_id for NFTs, item_id for items.
    fn token_or_item_id(&self) -> String {
        self.token_id
            .clone()
            .or_else(|| self.item_id.clone())
            .unwrap_or_default()
    }
}

/// Insert `rarity`/`nftName` only when present, matching upstream — those keys
/// are omitted (not null) when the asset has no rarity/name.
fn insert_opt(map: &mut serde_json::Map<String, serde_json::Value>, key: &str, v: &Option<String>) {
    if let Some(val) = v {
        map.insert(key.into(), serde_json::json!(val));
    }
}

/// Port of `fromBidAndAssetsToBidAcceptedEventNotification`. Requires exactly one
/// resolved asset; `bid.sent[0]` is the ERC20 MANA the bid offered.
fn bid_accepted_event(bid: &Trade, assets: &[&AssetMeta]) -> Option<serde_json::Value> {
    if assets.len() != 1 {
        return None;
    }
    let asset = assets[0];
    let price = bid
        .sent
        .first()
        .and_then(|a| a.amount.clone())
        .unwrap_or_default();
    let mut metadata = serde_json::Map::new();
    metadata.insert("address".into(), serde_json::json!(bid.signer));
    metadata.insert("image".into(), serde_json::json!(asset.image));
    metadata.insert("seller".into(), serde_json::json!(asset.seller));
    metadata.insert("category".into(), serde_json::json!(asset.category));
    insert_opt(&mut metadata, "rarity", &asset.rarity);
    metadata.insert("link".into(), serde_json::json!(asset.link()));
    insert_opt(&mut metadata, "nftName", &asset.name);
    metadata.insert("price".into(), serde_json::json!(price));
    metadata.insert("title".into(), serde_json::json!("Bid Accepted"));
    metadata.insert(
        "description".into(),
        serde_json::json!(format!(
            "Your bid for {} MANA for this {} was accepted.",
            crate::logic::numeric::format_ether(&price),
            asset.name.as_deref().unwrap_or_default()
        )),
    );
    metadata.insert("network".into(), serde_json::json!(bid.network));

    Some(serde_json::json!({
        "type": "blockchain",
        "subType": "bid-accepted",
        "key": format!("bid-accepted-{}", bid.id),
        // overwritten with the request timestamp by the caller; upstream uses Date.now().
        "timestamp": 0,
        "metadata": serde_json::Value::Object(metadata),
    }))
}

/// Port of `fromTradeAndAssetsToItemSoldEventNotification`. Requires exactly one
/// resolved asset; `caller` is the buyer.
fn item_sold_event(
    trade: &Trade,
    assets: &[&AssetMeta],
    caller: &str,
) -> Option<serde_json::Value> {
    if assets.len() != 1 {
        return None;
    }
    let asset = assets[0];
    let mut metadata = serde_json::Map::new();
    metadata.insert("address".into(), serde_json::json!(trade.signer));
    metadata.insert("image".into(), serde_json::json!(asset.image));
    metadata.insert("seller".into(), serde_json::json!(asset.seller));
    metadata.insert("buyer".into(), serde_json::json!(caller));
    metadata.insert("category".into(), serde_json::json!(asset.category));
    insert_opt(&mut metadata, "rarity", &asset.rarity);
    metadata.insert("link".into(), serde_json::json!(asset.link()));
    insert_opt(&mut metadata, "nftName", &asset.name);
    metadata.insert("title".into(), serde_json::json!("Item Sold"));
    metadata.insert(
        "description".into(),
        serde_json::json!(format!(
            "Someone just bought your {}",
            asset.name.as_deref().unwrap_or_default()
        )),
    );
    metadata.insert("network".into(), serde_json::json!(trade.network));
    metadata.insert(
        "tokenId".into(),
        serde_json::json!(asset.token_or_item_id()),
    );

    Some(serde_json::json!({
        "type": "blockchain",
        "subType": "item-sold",
        "key": format!("item-sold-{}", trade.id),
        "timestamp": 0,
        "metadata": serde_json::Value::Object(metadata),
    }))
}

#[cfg(test)]
mod wire_tests {
    use super::DbTradeListRow;
    use chrono::TimeZone;

    // Serializes the tests that mutate the process-global MARKETPLACE_BASE_URL so
    // they don't race under the parallel test runner.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn list_row_is_snake_case_with_iso_timestamps() {
        let ts = chrono::Utc
            .with_ymd_and_hms(2024, 3, 15, 12, 34, 56)
            .unwrap()
            + chrono::Duration::milliseconds(789);
        let row = DbTradeListRow {
            id: "abc".into(),
            chain_id: 137,
            checks: serde_json::json!({"uses": 1}),
            created_at: ts,
            effective_since: ts,
            expires_at: ts,
            network: "MATIC".into(),
            signature: "0xsig".into(),
            signer: "0xsigner".into(),
            trade_type: "public_nft_order".into(),
            contract: "0xcontract".into(),
        };
        let v = serde_json::to_value(&row).unwrap();
        let obj = v.as_object().unwrap();
        // snake_case keys (matches upstream DBTrade from `SELECT *`).
        assert!(obj.contains_key("chain_id"));
        assert!(obj.contains_key("created_at"));
        assert!(obj.contains_key("effective_since"));
        assert!(obj.contains_key("expires_at"));
        // no camelCase leakage.
        assert!(!obj.contains_key("chainId"));
        assert!(!obj.contains_key("createdAt"));
        assert!(!obj.contains_key("effectiveSince"));
        assert!(!obj.contains_key("expiresAt"));
        // `type` rename preserved.
        assert_eq!(obj.get("type").unwrap(), "public_nft_order");
        // ISO-8601 string timestamps with millisecond precision + Z suffix
        // (byte-identical to JS Date.toJSON()), not millisecond integers.
        assert_eq!(obj.get("created_at").unwrap(), "2024-03-15T12:34:56.789Z");
    }

    // --- #12: public `Trade` (`/v1/trades/{id}`) + `/accept` event wire shapes -

    use super::{
        bid_accepted_event, item_sold_event, AssetMeta, DbTrade, Trade, TradeAsset, TradeView,
    };

    fn sample_view(
        trade_type: &str,
        sent: Vec<TradeAsset>,
        received: Vec<TradeAsset>,
    ) -> TradeView {
        let ts = chrono::Utc
            .with_ymd_and_hms(2024, 3, 15, 12, 34, 56)
            .unwrap()
            + chrono::Duration::milliseconds(789);
        TradeView {
            trade: DbTrade {
                id: "trade-1".into(),
                chain_id: 137,
                checks: serde_json::json!({"uses": 1, "signerSignatureIndex": 0}),
                created_at: ts,
                effective_since: ts,
                expires_at: ts,
                network: "MATIC".into(),
                signature: "0xsig".into(),
                signer: "0xsigner".into(),
                trade_type: trade_type.into(),
                contract: "0xcontract".into(),
            },
            sent,
            received,
        }
    }

    fn db_asset(
        asset_type: i32,
        direction: &str,
        amount: Option<&str>,
        token_id: Option<&str>,
        item_id: Option<&str>,
        beneficiary: Option<&str>,
    ) -> TradeAsset {
        TradeAsset {
            asset_type,
            contract_address: "0xasset".into(),
            beneficiary: beneficiary.map(String::from),
            direction: direction.into(),
            extra: "0xextra".into(),
            amount: amount.map(String::from),
            token_id: token_id.map(String::from),
            item_id: item_id.map(String::from),
        }
    }

    #[test]
    fn public_trade_has_no_effective_since_or_expires_at() {
        // sent ERC20 (amount), received ERC721 (tokenId + beneficiary).
        let view = sample_view(
            "public_nft_order",
            vec![db_asset(1, "sent", Some("1000"), None, None, None)],
            vec![db_asset(
                3,
                "received",
                None,
                Some("42"),
                None,
                Some("0xben"),
            )],
        );
        let trade = Trade::from_view(&view);
        let v = serde_json::to_value(&trade).unwrap();
        let obj = v.as_object().unwrap();

        // Public Trade is camelCase with createdAt only.
        assert!(obj.contains_key("chainId"));
        assert!(obj.contains_key("createdAt"));
        // createdAt is a NUMBER (epoch ms), not an ISO string.
        assert_eq!(
            obj.get("createdAt").unwrap(),
            &serde_json::json!(1_710_506_096_789i64)
        );
        // The internal DBTrade-only fields are absent on the public Trade.
        assert!(!obj.contains_key("effectiveSince"));
        assert!(!obj.contains_key("effective_since"));
        assert!(!obj.contains_key("expiresAt"));
        assert!(!obj.contains_key("expires_at"));
        // exact public key set.
        let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "chainId",
                "checks",
                "contract",
                "createdAt",
                "id",
                "network",
                "received",
                "sent",
                "signature",
                "signer",
                "type"
            ]
        );
    }

    #[test]
    fn public_trade_assets_are_discriminated_union() {
        let view = sample_view(
            "bid",
            // sent ERC20 → amount only.
            vec![db_asset(1, "sent", Some("1000"), None, None, None)],
            // received COLLECTION_ITEM → itemId only, plus beneficiary.
            vec![db_asset(
                4,
                "received",
                None,
                None,
                Some("7"),
                Some("0xben"),
            )],
        );
        let trade = Trade::from_view(&view);
        let v = serde_json::to_value(&trade).unwrap();

        let sent = &v["sent"][0];
        // ERC20: amount present; tokenId / itemId / beneficiary / direction absent.
        assert_eq!(sent["assetType"], serde_json::json!(1));
        assert_eq!(sent["amount"], serde_json::json!("1000"));
        assert!(sent.get("tokenId").is_none());
        assert!(sent.get("itemId").is_none());
        assert!(sent.get("beneficiary").is_none());
        assert!(sent.get("direction").is_none());
        assert_eq!(sent["contractAddress"], serde_json::json!("0xasset"));
        assert_eq!(sent["extra"], serde_json::json!("0xextra"));

        let received = &v["received"][0];
        // COLLECTION_ITEM: itemId present; amount / tokenId absent; beneficiary set.
        assert_eq!(received["assetType"], serde_json::json!(4));
        assert_eq!(received["itemId"], serde_json::json!("7"));
        assert!(received.get("amount").is_none());
        assert!(received.get("tokenId").is_none());
        assert_eq!(received["beneficiary"], serde_json::json!("0xben"));
    }

    fn item_meta() -> AssetMeta {
        AssetMeta {
            image: "https://img/1.png".into(),
            seller: "0xcreator".into(),
            category: "wearable".into(),
            rarity: Some("mythic".into()),
            name: Some("Cool Hat".into()),
            contract_address: "0xcollection".into(),
            token_id: None,
            item_id: Some("7".into()),
        }
    }

    fn nft_meta() -> AssetMeta {
        AssetMeta {
            image: "https://img/42.png".into(),
            seller: "0xowner".into(),
            category: "wearable".into(),
            rarity: Some("epic".into()),
            name: Some("Rare Boots".into()),
            contract_address: "0xnftcontract".into(),
            token_id: Some("42".into()),
            item_id: None,
        }
    }

    #[test]
    fn accept_event_bid_accepted_shape() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("MARKETPLACE_BASE_URL", "https://market.example");
        // A BID: signer is the bidder, sent[0] is the ERC20 MANA offered.
        let view = sample_view(
            "bid",
            vec![db_asset(
                1,
                "sent",
                Some("1500000000000000000"),
                None,
                None,
                None,
            )],
            vec![db_asset(
                4,
                "received",
                None,
                None,
                Some("7"),
                Some("0xben"),
            )],
        );
        let trade = Trade::from_view(&view);
        let meta = item_meta();
        let ev = bid_accepted_event(&trade, &[&meta]).expect("event");

        assert_eq!(ev["type"], serde_json::json!("blockchain"));
        assert_eq!(ev["subType"], serde_json::json!("bid-accepted"));
        assert_eq!(ev["key"], serde_json::json!("bid-accepted-trade-1"));
        let md = &ev["metadata"];
        // bid-accepted: address is the bid signer.
        assert_eq!(md["address"], serde_json::json!("0xsigner"));
        assert_eq!(md["image"], serde_json::json!("https://img/1.png"));
        assert_eq!(md["seller"], serde_json::json!("0xcreator"));
        assert_eq!(md["category"], serde_json::json!("wearable"));
        assert_eq!(md["rarity"], serde_json::json!("mythic"));
        assert_eq!(
            md["link"],
            serde_json::json!("https://market.example/contracts/0xcollection/items/7")
        );
        assert_eq!(md["nftName"], serde_json::json!("Cool Hat"));
        assert_eq!(md["price"], serde_json::json!("1500000000000000000"));
        assert_eq!(md["title"], serde_json::json!("Bid Accepted"));
        assert_eq!(
            md["description"],
            serde_json::json!("Your bid for 1.5 MANA for this Cool Hat was accepted.")
        );
        assert_eq!(md["network"], serde_json::json!("MATIC"));
        // bid-accepted carries no buyer / tokenId.
        assert!(md.get("buyer").is_none());
        assert!(md.get("tokenId").is_none());
    }

    #[test]
    fn accept_event_item_sold_shape() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("MARKETPLACE_BASE_URL", "https://market.example");
        // A public_nft_order: signer is the seller, sent[0] is the NFT.
        let view = sample_view(
            "public_nft_order",
            vec![db_asset(3, "sent", None, Some("42"), None, None)],
            vec![db_asset(
                1,
                "received",
                Some("2000000000000000000"),
                None,
                None,
                Some("0xben"),
            )],
        );
        let trade = Trade::from_view(&view);
        let meta = nft_meta();
        let ev = item_sold_event(&trade, &[&meta], "0xbuyer").expect("event");

        assert_eq!(ev["type"], serde_json::json!("blockchain"));
        assert_eq!(ev["subType"], serde_json::json!("item-sold"));
        assert_eq!(ev["key"], serde_json::json!("item-sold-trade-1"));
        let md = &ev["metadata"];
        assert_eq!(md["address"], serde_json::json!("0xsigner"));
        assert_eq!(md["image"], serde_json::json!("https://img/42.png"));
        assert_eq!(md["seller"], serde_json::json!("0xowner"));
        // item-sold carries buyer = caller.
        assert_eq!(md["buyer"], serde_json::json!("0xbuyer"));
        assert_eq!(md["category"], serde_json::json!("wearable"));
        assert_eq!(md["rarity"], serde_json::json!("epic"));
        assert_eq!(
            md["link"],
            serde_json::json!("https://market.example/contracts/0xnftcontract/tokens/42")
        );
        assert_eq!(md["nftName"], serde_json::json!("Rare Boots"));
        assert_eq!(md["title"], serde_json::json!("Item Sold"));
        assert_eq!(
            md["description"],
            serde_json::json!("Someone just bought your Rare Boots")
        );
        assert_eq!(md["network"], serde_json::json!("MATIC"));
        // item-sold's tokenId is the NFT token id.
        assert_eq!(md["tokenId"], serde_json::json!("42"));
        // item-sold has no `price`.
        assert!(md.get("price").is_none());
    }

    #[test]
    fn accept_event_requires_exactly_one_resolved_asset() {
        let view = sample_view(
            "public_nft_order",
            vec![db_asset(3, "sent", None, Some("42"), None, None)],
            vec![db_asset(
                1,
                "received",
                Some("1000"),
                None,
                None,
                Some("0xben"),
            )],
        );
        let trade = Trade::from_view(&view);
        // Zero resolved assets → no event (upstream returns null → 500).
        assert!(item_sold_event(&trade, &[], "0xbuyer").is_none());
        // Two resolved assets → no event.
        let m1 = nft_meta();
        let m2 = item_meta();
        assert!(item_sold_event(&trade, &[&m1, &m2], "0xbuyer").is_none());
    }

    #[test]
    fn rarity_and_name_omitted_when_absent() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("MARKETPLACE_BASE_URL", "");
        let view = sample_view(
            "public_item_order",
            vec![db_asset(4, "sent", None, None, Some("9"), None)],
            vec![db_asset(
                1,
                "received",
                Some("1000"),
                None,
                None,
                Some("0xben"),
            )],
        );
        let trade = Trade::from_view(&view);
        let meta = AssetMeta {
            image: "img".into(),
            seller: "0xc".into(),
            category: "emote".into(),
            rarity: None,
            name: None,
            contract_address: "0xcollection".into(),
            token_id: None,
            item_id: Some("9".into()),
        };
        let ev = item_sold_event(&trade, &[&meta], "0xbuyer").expect("event");
        let md = ev["metadata"].as_object().unwrap();
        // upstream omits (not nulls) rarity / nftName when the asset lacks them.
        assert!(!md.contains_key("rarity"));
        assert!(!md.contains_key("nftName"));
    }
}
