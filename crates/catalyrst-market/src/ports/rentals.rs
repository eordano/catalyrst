use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct RentalListingPeriod {
    #[serde(rename = "minDays")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub min_days: i64,
    #[serde(rename = "maxDays")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub max_days: i64,
    #[serde(rename = "pricePerDay")]
    pub price_per_day: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct RentalListing {
    pub id: String,
    #[serde(rename = "nftId")]
    pub nft_id: String,
    pub category: String,
    #[serde(rename = "searchText")]
    pub search_text: String,
    pub network: String,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: i64,

    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expiration: i64,
    pub signature: String,
    pub nonces: Vec<String>,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "rentalContractAddress")]
    pub rental_contract_address: String,
    pub lessor: Option<String>,
    pub tenant: Option<String>,
    pub status: String,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[serde(rename = "startedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at: Option<i64>,
    pub periods: Vec<RentalListingPeriod>,
    pub target: String,
    #[serde(rename = "rentedDays")]
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub rented_days: Option<i64>,
}

#[derive(Clone)]
pub struct RentalsComponent {
    pool: Option<PgPool>,
}

impl RentalsComponent {
    pub fn new(pool: Option<PgPool>) -> Self {
        Self { pool }
    }

    pub fn is_enabled(&self) -> bool {
        self.pool.is_some()
    }

    pub async fn get_rentals_listings_of_nfts(
        &self,
        nft_ids: &[String],
        statuses: &[String],
    ) -> Vec<RentalListing> {
        let Some(pool) = &self.pool else {
            return Vec::new();
        };
        if nft_ids.is_empty() {
            return Vec::new();
        }
        let status_filter: Vec<String> = if statuses.is_empty() {
            vec!["open".to_string()]
        } else {
            statuses.to_vec()
        };

        let rows = sqlx::query(
            r#"
SELECT rentals.*, metadata.category, metadata.search_text,
       rl.lessor, rl.tenant, p.periods
FROM rentals
JOIN metadata ON metadata.id = rentals.metadata_id
JOIN rentals_listings rl ON rl.id = rentals.id
JOIN (
    SELECT periods.rental_id,
           jsonb_agg(
               jsonb_build_array(
                   periods.min_days::text,
                   periods.max_days::text,
                   periods.price_per_day::text
               ) ORDER BY periods.min_days
           ) AS periods
    FROM periods
    GROUP BY periods.rental_id
) p ON p.rental_id = rentals.id
WHERE rentals.metadata_id = ANY($1)
  AND rentals.status::text = ANY($2)
"#,
        )
        .bind(nft_ids)
        .bind(&status_filter)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        rows.iter().map(row_to_listing).collect()
    }

    pub async fn get_open_rentals(&self, statuses: &[String]) -> Vec<RentalListing> {
        let Some(pool) = &self.pool else {
            return Vec::new();
        };
        let status_filter: Vec<String> = if statuses.is_empty() {
            vec!["open".to_string()]
        } else {
            statuses.to_vec()
        };
        let rows = sqlx::query(
            r#"
SELECT rentals.*, metadata.category, metadata.search_text,
       rl.lessor, rl.tenant, p.periods
FROM rentals
JOIN metadata ON metadata.id = rentals.metadata_id
JOIN rentals_listings rl ON rl.id = rentals.id
JOIN (
    SELECT periods.rental_id,
           jsonb_agg(
               jsonb_build_array(
                   periods.min_days::text,
                   periods.max_days::text,
                   periods.price_per_day::text
               ) ORDER BY periods.min_days
           ) AS periods
    FROM periods
    GROUP BY periods.rental_id
) p ON p.rental_id = rentals.id
WHERE rentals.status::text = ANY($1)
ORDER BY rentals.created_at DESC
LIMIT 1000
"#,
        )
        .bind(&status_filter)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        rows.iter().map(row_to_listing).collect()
    }

    pub async fn get_rental_assets_ids_for_lessor(&self, owner: &str) -> Vec<String> {
        let Some(pool) = &self.pool else {
            return Vec::new();
        };
        let rows = sqlx::query(
            r#"
SELECT DISTINCT rentals.metadata_id AS nft_id
FROM rentals
JOIN rentals_listings rl ON rl.id = rentals.id
WHERE LOWER(rl.lessor) = LOWER($1)
  AND rentals.status = 'open'
"#,
        )
        .bind(owner)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        rows.iter()
            .filter_map(|r| r.try_get::<String, _>("nft_id").ok())
            .collect()
    }
}

fn to_millis(dt: NaiveDateTime) -> i64 {
    dt.and_utc().timestamp_millis()
}

fn row_to_listing(r: &sqlx::postgres::PgRow) -> RentalListing {
    let periods_raw: serde_json::Value = r.try_get("periods").unwrap_or(serde_json::Value::Null);
    let periods = periods_raw
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    if p.len() < 3 {
                        return None;
                    }
                    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
                    Some(RentalListingPeriod {
                        min_days: s(&p[0]).parse().unwrap_or(0),
                        max_days: s(&p[1]).parse().unwrap_or(0),
                        price_per_day: s(&p[2]),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let started_at: Option<NaiveDateTime> = r.try_get("started_at").ok().flatten();
    let rented_days: Option<i32> = r.try_get("rented_days").ok().flatten();

    RentalListing {
        id: r
            .try_get::<Uuid, _>("id")
            .map(|u| u.to_string())
            .unwrap_or_default(),
        nft_id: r.try_get("metadata_id").unwrap_or_default(),
        category: r.try_get("category").unwrap_or_default(),
        search_text: r.try_get("search_text").unwrap_or_default(),
        network: r.try_get("network").unwrap_or_default(),
        chain_id: r.try_get::<i32, _>("chain_id").unwrap_or(0) as i64,
        expiration: r
            .try_get::<NaiveDateTime, _>("expiration")
            .map(to_millis)
            .unwrap_or(0),
        signature: r.try_get("signature").unwrap_or_default(),
        nonces: r.try_get("nonces").unwrap_or_default(),
        token_id: r.try_get("token_id").unwrap_or_default(),
        contract_address: r.try_get("contract_address").unwrap_or_default(),
        rental_contract_address: r.try_get("rental_contract_address").unwrap_or_default(),
        lessor: r.try_get("lessor").ok().flatten(),
        tenant: r.try_get("tenant").ok().flatten(),
        status: r.try_get("status").unwrap_or_else(|_| "open".to_string()),
        created_at: r
            .try_get::<NaiveDateTime, _>("created_at")
            .map(to_millis)
            .unwrap_or(0),
        updated_at: r
            .try_get::<NaiveDateTime, _>("updated_at")
            .map(to_millis)
            .unwrap_or(0),
        started_at: started_at.map(to_millis),
        periods,
        target: r.try_get("target").unwrap_or_default(),
        rented_days: rented_days.map(|d| d as i64),
    }
}
