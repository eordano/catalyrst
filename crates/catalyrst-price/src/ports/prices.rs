use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPool, Row};

#[derive(Debug, Clone)]
pub struct PriceSnapshot {
    pub mana_usd: Option<f64>,
    pub mana_eth: Option<f64>,
    pub mana_btc: Option<f64>,
    pub market_cap_usd: Option<f64>,
    pub volume_24h_usd: Option<f64>,
    pub price_change_24h_pct: Option<f64>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub taken_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct PricesComponent {
    pool: PgPool,
}

impl PricesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn latest_coingecko(&self) -> Result<Option<PriceSnapshot>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT mana_usd::double precision AS mana_usd, \
                    mana_eth::double precision AS mana_eth, \
                    mana_btc::double precision AS mana_btc, \
                    mana_market_cap_usd::double precision AS market_cap_usd, \
                    mana_volume_24h_usd::double precision AS volume_24h_usd, \
                    mana_price_change_24h_pct::double precision AS price_change_24h_pct, \
                    source_updated_at, \
                    taken_at \
             FROM price_snapshots \
             WHERE source = 'coingecko' \
             ORDER BY taken_at DESC \
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| PriceSnapshot {
            mana_usd: r.get("mana_usd"),
            mana_eth: r.get("mana_eth"),
            mana_btc: r.get("mana_btc"),
            market_cap_usd: r.get("market_cap_usd"),
            volume_24h_usd: r.get("volume_24h_usd"),
            price_change_24h_pct: r.get("price_change_24h_pct"),
            source_updated_at: r.get("source_updated_at"),
            taken_at: r.get("taken_at"),
        }))
    }
}
