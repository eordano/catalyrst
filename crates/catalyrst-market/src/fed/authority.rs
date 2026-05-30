use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

pub async fn signer_owns_any_nft_for_item(
    pool: &PgPool,
    signer: &str,
    item_id: &str,
) -> Result<bool, ApiError> {
    let sql = format!(
        "SELECT 1 FROM {schema}.nft nft \
           JOIN {schema}.account account ON nft.owner_id = account.id \
          WHERE LOWER(nft.item_id) = LOWER($1) \
            AND LOWER(account.address) = LOWER($2) \
          LIMIT 1",
        schema = MARKETPLACE_SQUID_SCHEMA
    );
    let row: Option<(i32,)> = sqlx::query_as(&sql)
        .bind(item_id)
        .bind(signer)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::from)?;
    Ok(row.is_some())
}

pub async fn lookup_bid_item_id(pool: &PgPool, bid_sig_hash: &str) -> Result<Option<String>, ApiError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT item_id FROM market_bids_local WHERE signature_hash = $1",
    )
    .bind(bid_sig_hash)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;
    Ok(row.map(|(s,)| s))
}

pub async fn lookup_bid_signer(pool: &PgPool, bid_sig_hash: &str) -> Result<Option<String>, ApiError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT signer FROM market_bids_local WHERE signature_hash = $1",
    )
    .bind(bid_sig_hash)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;
    Ok(row.map(|(s,)| s))
}

pub async fn lookup_order_signer(pool: &PgPool, order_sig_hash: &str) -> Result<Option<String>, ApiError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT signer FROM market_orders_local WHERE signature_hash = $1",
    )
    .bind(order_sig_hash)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;
    Ok(row.map(|(s,)| s))
}

pub async fn order_exists(pool: &PgPool, order_sig_hash: &str) -> Result<bool, ApiError> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM market_orders_local WHERE signature_hash = $1",
    )
    .bind(order_sig_hash)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;
    Ok(row.is_some())
}
