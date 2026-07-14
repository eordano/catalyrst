use sqlx::PgPool;

use super::sql::{
    build_importable_listings_sql, build_legacy_listings_sql, build_shop_listings_sql, to_credits,
    Bind,
};
use super::types::{
    ImportableListing, ImportableListingRow, LegacyCatalogFilters, LegacyListing, LegacyListingRow,
    ShopCatalogFilters, ShopListing, ShopListingRow,
};
use crate::dcl_schemas::{ethereum_chain_id, polygon_chain_id, ChainId, Network};
use crate::http::response::ApiError;

pub(super) fn top_level_category(item_type: Option<&str>) -> &'static str {
    if item_type
        .map(|t| t.to_lowercase().starts_with("emote"))
        .unwrap_or(false)
    {
        "emote"
    } else {
        "wearable"
    }
}

pub(super) fn network_and_chain(raw: Option<&str>) -> (Network, ChainId) {
    if raw.unwrap_or("MATIC").eq_ignore_ascii_case("ETHEREUM") {
        (Network::Ethereum, ethereum_chain_id())
    } else {
        (Network::Matic, polygon_chain_id())
    }
}

pub(super) fn listing_type(trade_type: &str) -> &'static str {
    if trade_type == "public_item_order" {
        "primary"
    } else {
        "secondary"
    }
}

pub(super) fn parse_available(available: Option<&str>) -> i64 {
    available.and_then(|s| s.parse::<i64>().ok()).unwrap_or(1)
}

pub struct ShopCatalogComponent {
    pool: PgPool,
}

impl ShopCatalogComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(super) async fn fetch<R>(&self, sql: String, binds: Vec<Bind>) -> Result<Vec<R>, ApiError>
    where
        R: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let mut q = sqlx::query_as::<_, R>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = match b {
                Bind::Text(s) => q.bind(s.clone()),
                Bind::TextArray(v) => q.bind(v.clone()),
                Bind::Int(i) => q.bind(*i),
            };
        }
        Ok(q.fetch_all(&self.pool).await?)
    }

    pub async fn get_shop_listings(
        &self,
        filters: &ShopCatalogFilters,
    ) -> Result<(Vec<ShopListing>, i64), ApiError> {
        let (sql, binds) = build_shop_listings_sql(filters);
        let rows: Vec<ShopListingRow> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.total).unwrap_or(0);

        let mut data = Vec::with_capacity(rows.len());
        for r in rows {
            let Some(price_credits) = r.price.as_deref().and_then(to_credits) else {
                tracing::warn!(
                    trade_id = %r.trade_id,
                    price = r.price.as_deref().unwrap_or(""),
                    "dropping shop listing with non-positive or unparseable price"
                );
                continue;
            };
            let (network, chain_id) = network_and_chain(r.network.as_deref());
            data.push(ShopListing {
                trade_id: r.trade_id,
                listing_type: listing_type(&r.trade_type).to_string(),
                contract_address: r.contract_address.unwrap_or_default(),
                item_id: r.item_id,
                token_id: r.token_id,
                name: r.name.unwrap_or_default(),
                thumbnail: r.image.unwrap_or_default(),
                rarity: r.rarity.as_deref().unwrap_or("common").to_lowercase(),
                category: top_level_category(r.item_type.as_deref()).to_string(),
                wearable_category: r.wearable_category,
                gender: r.gender,
                creator: r.creator.unwrap_or_default(),
                price_credits,
                available: parse_available(r.available.as_deref()),
                network,
                chain_id,
                created_at: r.created_at,
            });
        }
        Ok((data, total))
    }

    // A seller's OPEN classic (ERC20-MANA) listings — the "old liquidity" they can import into
    // the Shop. Price is returned raw (MANA wei); the client converts to credits via the oracle.
    pub async fn get_importable_listings(
        &self,
        seller: &str,
    ) -> Result<Vec<ImportableListing>, ApiError> {
        let (sql, binds) = build_importable_listings_sql(seller);
        let rows: Vec<ImportableListingRow> = self.fetch(sql, binds).await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let (network, chain_id) = network_and_chain(r.network.as_deref());
                ImportableListing {
                    old_trade_id: r.old_trade_id,
                    listing_type: listing_type(&r.trade_type).to_string(),
                    contract_address: r.contract_address.unwrap_or_default(),
                    item_id: r.item_id,
                    token_id: r.token_id,
                    name: r.name.unwrap_or_default(),
                    thumbnail: r.image.unwrap_or_default(),
                    rarity: r.rarity.as_deref().unwrap_or("common").to_lowercase(),
                    category: top_level_category(r.item_type.as_deref()).to_string(),
                    wearable_category: r.wearable_category,
                    mana_wei: r.mana_wei.unwrap_or_default(),
                    available: parse_available(r.available.as_deref()),
                    network,
                    chain_id,
                }
            })
            .collect())
    }

    pub async fn get_legacy_listings(
        &self,
        filters: &LegacyCatalogFilters,
    ) -> Result<(Vec<LegacyListing>, i64), ApiError> {
        let (sql, binds) = build_legacy_listings_sql(filters);
        let rows: Vec<LegacyListingRow> = self.fetch(sql, binds).await?;
        let total = rows.first().map(|r| r.total).unwrap_or(0);

        let data = rows
            .into_iter()
            .map(|r| {
                let (network, chain_id) = network_and_chain(r.network.as_deref());
                LegacyListing {
                    trade_id: r.trade_id,
                    listing_type: "primary".to_string(),
                    contract_address: r.contract_address.unwrap_or_default(),
                    item_id: r.item_id,
                    name: r.name.unwrap_or_default(),
                    thumbnail: r.image.unwrap_or_default(),
                    rarity: r.rarity.as_deref().unwrap_or("common").to_lowercase(),
                    category: top_level_category(r.item_type.as_deref()).to_string(),
                    wearable_category: r.wearable_category,
                    gender: r.gender,
                    creator: r.creator.unwrap_or_default(),
                    mana_wei: r.mana_wei.unwrap_or_default(),
                    available: parse_available(r.available.as_deref()),
                    network,
                    chain_id,
                    created_at: r.created_at,
                }
            })
            .collect();
        Ok((data, total))
    }
}
