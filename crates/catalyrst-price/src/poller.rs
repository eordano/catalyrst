use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use sqlx::postgres::PgPool;

use crate::config::Config;

const MANA_ID: &str = "decentraland";
const MATIC_ID: &str = "matic-network";
const SOURCE: &str = "coingecko";

#[derive(Debug, Default, Deserialize)]
struct CoinQuote {
    usd: Option<f64>,
    eth: Option<f64>,
    btc: Option<f64>,
    usd_market_cap: Option<f64>,
    usd_24h_vol: Option<f64>,
    usd_24h_change: Option<f64>,
    last_updated_at: Option<i64>,
}

type SimplePriceResponse = HashMap<String, CoinQuote>;

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotRow {
    pub source_updated_at: Option<DateTime<Utc>>,
    pub mana_usd: Option<f64>,
    pub mana_eth: Option<f64>,
    pub mana_btc: Option<f64>,
    pub mana_matic: Option<f64>,
    pub matic_usd: Option<f64>,
    pub mana_market_cap_usd: Option<f64>,
    pub mana_volume_24h_usd: Option<f64>,
    pub mana_price_change_24h_pct: Option<f64>,
}

fn map_snapshot(resp: &SimplePriceResponse) -> SnapshotRow {
    let default = CoinQuote::default();
    let mana = resp.get(MANA_ID).unwrap_or(&default);
    let matic = resp.get(MATIC_ID).unwrap_or(&default);

    let mana_matic = match (mana.usd, matic.usd) {
        (Some(m), Some(t)) if t != 0.0 => Some(m / t),
        _ => None,
    };

    let source_updated_at = mana
        .last_updated_at
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single());

    SnapshotRow {
        source_updated_at,
        mana_usd: mana.usd,
        mana_eth: mana.eth,
        mana_btc: mana.btc,
        mana_matic,
        matic_usd: matic.usd,
        mana_market_cap_usd: mana.usd_market_cap,
        mana_volume_24h_usd: mana.usd_24h_vol,
        mana_price_change_24h_pct: mana.usd_24h_change,
    }
}

async fn fetch_snapshot(client: &reqwest::Client, base: &str) -> anyhow::Result<SnapshotRow> {
    let url = format!(
        "{}/simple/price?ids={},{}&vs_currencies=usd,eth,btc\
         &include_market_cap=true&include_24hr_vol=true\
         &include_24hr_change=true&include_last_updated_at=true",
        base, MANA_ID, MATIC_ID,
    );
    let resp: SimplePriceResponse = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(map_snapshot(&resp))
}

async fn insert_snapshot(pool: &PgPool, row: &SnapshotRow) -> Result<i64, sqlx::Error> {
    let rec = sqlx::query(
        "INSERT INTO price_snapshots \
            (source, source_updated_at, mana_usd, mana_eth, mana_btc, mana_matic, \
             matic_usd, mana_market_cap_usd, mana_volume_24h_usd, \
             mana_price_change_24h_pct) \
         VALUES ($1, $2, \
             $3::double precision, $4::double precision, $5::double precision, \
             $6::double precision, $7::double precision, $8::double precision, \
             $9::double precision, $10::double precision) \
         RETURNING id",
    )
    .bind(SOURCE)
    .bind(row.source_updated_at)
    .bind(row.mana_usd)
    .bind(row.mana_eth)
    .bind(row.mana_btc)
    .bind(row.mana_matic)
    .bind(row.matic_usd)
    .bind(row.mana_market_cap_usd)
    .bind(row.mana_volume_24h_usd)
    .bind(row.mana_price_change_24h_pct)
    .fetch_one(pool)
    .await?;
    use sqlx::Row;
    Ok(rec.get::<i64, _>("id"))
}

pub fn spawn(pool: PgPool, cfg: &Config) {
    let base = cfg.coingecko_url.clone();
    let interval = Duration::from_secs(cfg.price_poll_interval_secs.max(1));

    let client = match reqwest::Client::builder()
        .user_agent("catalyrst-price/0.1 mana-price-poller")
        .timeout(Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to build poll HTTP client; poll disabled");
            return;
        }
    };

    tokio::spawn(async move {
        tracing::info!(
            base = %base,
            interval_secs = interval.as_secs(),
            "mana-price poll task starting"
        );
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match fetch_snapshot(&client, &base).await {
                Ok(row) => match insert_snapshot(&pool, &row).await {
                    Ok(id) => tracing::info!(
                        snapshot_id = id,
                        mana_usd = ?row.mana_usd,
                        mana_eth = ?row.mana_eth,
                        mana_btc = ?row.mana_btc,
                        mana_matic = ?row.mana_matic,
                        "wrote mana_price snapshot"
                    ),
                    Err(e) => tracing::error!(error = %e, "failed to insert snapshot; will retry"),
                },
                Err(e) => {
                    tracing::warn!(error = %e, "coingecko fetch failed; will retry next cycle")
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_simple_price_payload_to_snapshot_row() {
        let json = serde_json::json!({
            "decentraland": {
                "usd": 0.42,
                "eth": 0.00012,
                "btc": 0.0000065,
                "usd_market_cap": 850000000.0,
                "usd_24h_vol": 12345678.0,
                "usd_24h_change": -3.21,
                "last_updated_at": 1_700_000_000_i64
            },
            "matic-network": {
                "usd": 0.84,
                "last_updated_at": 1_700_000_001_i64
            }
        });
        let resp: SimplePriceResponse = serde_json::from_value(json).unwrap();
        let row = map_snapshot(&resp);

        assert_eq!(row.mana_usd, Some(0.42));
        assert_eq!(row.mana_eth, Some(0.00012));
        assert_eq!(row.mana_btc, Some(0.0000065));
        assert_eq!(row.matic_usd, Some(0.84));
        assert_eq!(row.mana_market_cap_usd, Some(850000000.0));
        assert_eq!(row.mana_volume_24h_usd, Some(12345678.0));
        assert_eq!(row.mana_price_change_24h_pct, Some(-3.21));

        assert_eq!(row.mana_matic, Some(0.5));
        assert_eq!(
            row.source_updated_at,
            Some(Utc.timestamp_opt(1_700_000_000, 0).single().unwrap())
        );
    }

    #[test]
    fn mana_matic_is_none_when_matic_missing_or_zero() {
        let resp: SimplePriceResponse = serde_json::from_value(serde_json::json!({
            "decentraland": { "usd": 0.42 }
        }))
        .unwrap();
        assert_eq!(map_snapshot(&resp).mana_matic, None);

        let resp: SimplePriceResponse = serde_json::from_value(serde_json::json!({
            "decentraland": { "usd": 0.42 },
            "matic-network": { "usd": 0.0 }
        }))
        .unwrap();
        assert_eq!(map_snapshot(&resp).mana_matic, None);
    }

    #[test]
    fn missing_decentraland_yields_all_none() {
        let resp: SimplePriceResponse = serde_json::from_value(serde_json::json!({})).unwrap();
        let row = map_snapshot(&resp);
        assert_eq!(row.mana_usd, None);
        assert_eq!(row.mana_matic, None);
        assert_eq!(row.source_updated_at, None);
    }
}
