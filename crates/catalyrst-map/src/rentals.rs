use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentalPeriod {
    #[serde(rename = "minDays")]
    pub min_days: i64,
    #[serde(rename = "maxDays")]
    pub max_days: i64,
    #[serde(rename = "pricePerDay")]
    pub price_per_day: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TileRentalListing {
    pub expiration: i64,
    pub periods: Vec<RentalPeriod>,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

impl TileRentalListing {
    pub fn max_price_per_day(&self) -> String {
        let mut best = "0".to_string();
        for p in &self.periods {
            if numeric_str_gt(&p.price_per_day, &best) {
                best = p.price_per_day.clone();
            }
        }
        best
    }
}

fn numeric_str_gt(a: &str, b: &str) -> bool {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    if a.len() != b.len() {
        return a.len() > b.len();
    }
    a > b
}

#[derive(Debug, Deserialize)]
struct RentalListingRaw {
    #[serde(rename = "nftId")]
    nft_id: Option<String>,
    expiration: Option<i64>,
    #[serde(default)]
    periods: Vec<RentalPeriod>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SignaturesData {
    #[serde(default)]
    results: Vec<RentalListingRaw>,
    #[serde(default)]
    total: i64,
}

#[derive(Debug, Deserialize)]
struct SignaturesResponse {
    #[serde(default)]
    data: Option<SignaturesData>,
}

#[derive(Clone)]
pub struct RentalsClient {
    client: reqwest::Client,
    base_url: String,
}

impl RentalsClient {
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("SIGNATURES_SERVER_URL")
            .or_else(|_| std::env::var("RENTALS_SIGNATURES_SERVER_URL"))
            .ok()
            .map(|s| s.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("catalyrst-map")
            .build()
            .ok()?;
        Some(Self { client, base_url })
    }

    pub async fn fetch_open_listings(&self) -> anyhow::Result<HashMap<String, TileRentalListing>> {
        let limit: i64 = 100;
        let mut offset: i64 = 0;
        let mut out: HashMap<String, TileRentalListing> = HashMap::new();
        loop {
            let url = format!(
                "{}/v1/rentals-listings?rentalStatus=open&limit={}&offset={}",
                self.base_url, limit, offset
            );
            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                anyhow::bail!("signatures server responded with {}", resp.status());
            }
            let parsed: SignaturesResponse = resp.json().await?;
            let Some(data) = parsed.data else {
                break;
            };
            let total = data.total;
            let count = data.results.len() as i64;
            for r in data.results {
                let Some(nft_id) = r.nft_id else { continue };
                out.insert(
                    nft_id,
                    TileRentalListing {
                        expiration: r.expiration.unwrap_or(0),
                        periods: r.periods,
                        updated_at: r.updated_at.unwrap_or(0),
                    },
                );
            }
            offset += count;
            if count == 0 || offset >= total {
                break;
            }
        }
        Ok(out)
    }
}
