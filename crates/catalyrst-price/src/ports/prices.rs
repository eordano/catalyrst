use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use sqlx::postgres::PgRow;
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

// Column list shared by the poller's RETURNING and the fallback SELECT; a
// macro so both stay `&'static str` literals for sqlx.
macro_rules! snapshot_columns {
    () => {
        "mana_usd::double precision AS mana_usd, \
         mana_eth::double precision AS mana_eth, \
         mana_btc::double precision AS mana_btc, \
         mana_market_cap_usd::double precision AS market_cap_usd, \
         mana_volume_24h_usd::double precision AS volume_24h_usd, \
         mana_price_change_24h_pct::double precision AS price_change_24h_pct, \
         source_updated_at, \
         taken_at"
    };
}
pub(crate) use snapshot_columns;

pub fn snapshot_from_row(r: &PgRow) -> PriceSnapshot {
    PriceSnapshot {
        mana_usd: r.get("mana_usd"),
        mana_eth: r.get("mana_eth"),
        mana_btc: r.get("mana_btc"),
        market_cap_usd: r.get("market_cap_usd"),
        volume_24h_usd: r.get("volume_24h_usd"),
        price_change_24h_pct: r.get("price_change_24h_pct"),
        source_updated_at: r.get("source_updated_at"),
        taken_at: r.get("taken_at"),
    }
}

#[derive(Clone)]
pub struct PricesComponent {
    pool: PgPool,
    live: Arc<RwLock<Option<Arc<PriceSnapshot>>>>,
    serve_live: bool,
}

impl PricesComponent {
    // serve_live: the in-process poller publishes here, so reads skip the DB
    // once a snapshot exists; false keeps every read on the DB row (external ingester).
    pub fn new(pool: PgPool, serve_live: bool) -> Self {
        Self {
            pool,
            live: Arc::new(RwLock::new(None)),
            serve_live,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn publish(&self, snapshot: PriceSnapshot) {
        *self.live.write().unwrap() = Some(Arc::new(snapshot));
    }

    pub fn live(&self) -> Option<Arc<PriceSnapshot>> {
        self.live.read().unwrap().clone()
    }

    pub async fn latest(&self) -> Result<Option<Arc<PriceSnapshot>>, sqlx::Error> {
        if self.serve_live {
            if let Some(snapshot) = self.live() {
                return Ok(Some(snapshot));
            }
        }
        let row = self.latest_coingecko().await?.map(Arc::new);
        if self.serve_live {
            if let Some(snapshot) = &row {
                let mut live = self.live.write().unwrap();
                if live.is_none() {
                    *live = Some(snapshot.clone());
                }
            }
        }
        Ok(row)
    }

    pub async fn latest_coingecko(&self) -> Result<Option<PriceSnapshot>, sqlx::Error> {
        let row = sqlx::query(concat!(
            "SELECT ",
            snapshot_columns!(),
            " FROM price_snapshots WHERE source = 'coingecko' ORDER BY taken_at DESC LIMIT 1"
        ))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().map(snapshot_from_row))
    }
}
