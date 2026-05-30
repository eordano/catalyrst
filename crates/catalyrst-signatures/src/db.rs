//! Postgres access layer. Ports the data-access half of
//! `src/ports/rentals/component.ts` and `queries/`.

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::types::{
    PaginatedListings, RentalListing, RentalListingCreation, RentalListingPeriod,
};

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

/// Filters accepted by GET /v1/rentals-listings — RentalsListingsFilterBy.
#[derive(Debug, Default)]
pub struct ListingFilters {
    pub category: Option<String>,
    pub text: Option<String>,
    pub lessor: Option<String>,
    pub tenant: Option<String>,
    pub status: Vec<String>,
    pub token_id: Option<String>,
    pub contract_addresses: Vec<String>,
    pub nft_ids: Vec<String>,
    pub network: Option<String>,
    pub updated_after: Option<i64>,
    pub target: Option<String>,
    pub min_price_per_day: Option<String>,
    pub max_price_per_day: Option<String>,
    pub min_distance_to_plaza: Option<i32>,
    pub max_distance_to_plaza: Option<i32>,
    pub min_estate_size: Option<i32>,
    pub max_estate_size: Option<i32>,
    pub adjacent_to_road: Option<bool>,
    pub rental_days: Vec<i32>,
}

pub struct ListingQuery {
    pub sort_by: Option<String>,
    pub sort_direction: Option<String>,
    pub offset: i64,
    pub limit: i64,
    pub filter: ListingFilters,
    pub history: bool,
}

/// Prices filter — GetRentalListingsPricesFilters.
#[derive(Debug, Default)]
pub struct PriceFilters {
    pub category: Option<String>,
    pub adjacent_to_road: Option<bool>,
    pub min_distance_to_plaza: Option<i32>,
    pub max_distance_to_plaza: Option<i32>,
    pub min_estate_size: Option<i32>,
    pub max_estate_size: Option<i32>,
    pub rental_days: Vec<i32>,
}

fn to_millis(ndt: NaiveDateTime) -> i64 {
    DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc).timestamp_millis()
}

impl Database {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a freshly-signed listing (metadata + rentals + rentals_listings +
    /// periods) in a single transaction. Mirrors createRentalListing's INSERTs.
    /// `nft` carries the (optionally subgraph-resolved) metadata; when no
    /// subgraph is configured we synthesize a minimal metadata row keyed by
    /// `<contract>-<tokenId>`.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_listing(
        &self,
        nft_id: &str,
        category: &str,
        search_text: &str,
        distance_to_plaza: Option<i32>,
        adjacent_to_road: Option<bool>,
        estate_size: Option<i32>,
        nft_created_at: NaiveDateTime,
        nft_updated_at: NaiveDateTime,
        rental: &RentalListingCreation,
        lessor: &str,
    ) -> Result<RentalListing, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"INSERT INTO metadata (id, category, search_text, distance_to_plaza, adjacent_to_road, estate_size, updated_at, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (id) DO UPDATE SET search_text = $3"#,
        )
        .bind(nft_id)
        .bind(category)
        .bind(search_text)
        .bind(distance_to_plaza)
        .bind(adjacent_to_road)
        .bind(estate_size)
        .bind(nft_updated_at)
        .bind(nft_created_at)
        .execute(&mut *tx)
        .await?;

        let expiration = DateTime::<Utc>::from_timestamp_millis(rental.expiration)
            .map(|d| d.naive_utc())
            .unwrap_or_else(|| Utc::now().naive_utc());

        let rental_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO rentals (metadata_id, network, chain_id, expiration, signature, nonces, token_id, contract_address, rental_contract_address, status, target)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'open',$10) RETURNING id"#,
        )
        .bind(nft_id)
        .bind(&rental.network)
        .bind(rental.chain_id as i32)
        .bind(expiration)
        .bind(&rental.signature)
        .bind(&rental.nonces)
        .bind(&rental.token_id)
        .bind(&rental.contract_address)
        .bind(&rental.rental_contract_address)
        .bind(&rental.target)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query("INSERT INTO rentals_listings (id, lessor) VALUES ($1,$2)")
            .bind(rental_id)
            .bind(lessor)
            .execute(&mut *tx)
            .await?;

        for p in &rental.periods {
            sqlx::query(
                "INSERT INTO periods (min_days, max_days, price_per_day, rental_id) VALUES ($1,$2,$3::numeric,$4)",
            )
            .bind(p.min_days as i32)
            .bind(p.max_days as i32)
            .bind(&p.price_per_day)
            .bind(rental_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Re-read as a single canonical listing row.
        self.get_listing_by_id(&rental_id.to_string())
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Detect the unique-open-rental violation so the handler can map it to 409.
    pub fn is_open_conflict(err: &sqlx::Error) -> bool {
        if let sqlx::Error::Database(db) = err {
            return db
                .constraint()
                .map(|c| c == "rentals_token_id_contract_address_status_unique_index")
                .unwrap_or(false);
        }
        false
    }

    /// Refresh a listing's metadata row from freshly-resolved NFT facts (ports
    /// the metadata half of `refreshRentalListing`). Keyed by the rental's
    /// `metadata_id` (which is `<contract>-<tokenId>`). Returns the number of
    /// rows updated (0 if the listing's metadata row no longer exists).
    #[allow(clippy::too_many_arguments)]
    pub async fn update_metadata_for_rental(
        &self,
        rental_id: &str,
        category: &str,
        search_text: &str,
        distance_to_plaza: Option<i32>,
        adjacent_to_road: Option<bool>,
        estate_size: Option<i32>,
        nft_updated_at: NaiveDateTime,
    ) -> Result<u64, sqlx::Error> {
        let uuid = match Uuid::parse_str(rental_id) {
            Ok(u) => u,
            Err(_) => return Ok(0),
        };
        let res = sqlx::query(
            r#"UPDATE metadata m SET
                   category = $2,
                   search_text = $3,
                   distance_to_plaza = $4,
                   adjacent_to_road = $5,
                   estate_size = $6,
                   updated_at = $7
               FROM rentals r
               WHERE r.id = $1 AND r.metadata_id = m.id"#,
        )
        .bind(uuid)
        .bind(category)
        .bind(search_text)
        .bind(distance_to_plaza)
        .bind(adjacent_to_road)
        .bind(estate_size)
        .bind(nft_updated_at)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn get_listing_by_id(
        &self,
        id: &str,
    ) -> Result<Option<RentalListing>, sqlx::Error> {
        let uuid = match Uuid::parse_str(id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let row = sqlx::query(LISTING_BY_ID_SQL)
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| row_to_listing(&r)))
    }

    pub async fn get_listings(
        &self,
        q: &ListingQuery,
    ) -> Result<PaginatedListings, sqlx::Error> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "SELECT rentals.*, metadata.category, metadata.search_text, \
             metadata.created_at as metadata_created_at, COUNT(*) OVER() as rentals_listings_count \
             FROM metadata, (SELECT ",
        );
        if !q.history {
            qb.push("DISTINCT ON (rentals.metadata_id) ");
        }
        qb.push(
            "rentals.*, rentals_listings.tenant, rentals_listings.lessor, \
             jsonb_agg(jsonb_build_array(periods.min_days::text, periods.max_days::text, periods.price_per_day::text) ORDER BY periods.min_days) as periods, \
             min(periods.price_per_day) as min_price_per_day, \
             max(periods.price_per_day) as max_price_per_day \
             FROM rentals, rentals_listings, periods ",
        );

        let f = &q.filter;

        // rentalDays sub-select join (CROSS reference applied in WHERE below).
        if !f.rental_days.is_empty() {
            qb.push(", (SELECT DISTINCT rental_id FROM periods WHERE ");
            let mut sep = qb.separated(" OR ");
            for d in &f.rental_days {
                sep.push("(min_days <= ");
                sep.push_bind_unseparated(*d);
                sep.push_unseparated(" AND max_days >= ");
                sep.push_bind_unseparated(*d);
                sep.push_unseparated(")");
            }
            qb.push(") as rental_days_periods ");
        }

        qb.push("WHERE rentals.id = rentals_listings.id AND periods.rental_id = rentals.id ");
        if !f.rental_days.is_empty() {
            qb.push("AND rental_days_periods.rental_id = rentals.id ");
        }
        if !f.status.is_empty() {
            qb.push("AND rentals.status = ANY(")
                .push_bind(f.status.clone())
                .push("::rental_status[]) ");
        }
        if let Some(t) = &f.target {
            qb.push("AND rentals.target = ").push_bind(t.clone()).push(" ");
        }
        if let Some(ua) = f.updated_after {
            let dt = DateTime::<Utc>::from_timestamp_millis(ua)
                .map(|d| d.naive_utc())
                .unwrap_or_else(|| Utc::now().naive_utc());
            qb.push("AND rentals.updated_at > ").push_bind(dt).push(" ");
        }
        if let Some(t) = &f.token_id {
            qb.push("AND rentals.token_id = ").push_bind(t.clone()).push(" ");
        }
        if !f.contract_addresses.is_empty() {
            qb.push("AND rentals.contract_address = ANY(")
                .push_bind(f.contract_addresses.clone())
                .push(") ");
        }
        if let Some(n) = &f.network {
            qb.push("AND rentals.network = ").push_bind(n.clone()).push(" ");
        }
        if let Some(l) = &f.lessor {
            qb.push("AND rentals_listings.lessor = ").push_bind(l.clone()).push(" ");
        }
        if let Some(t) = &f.tenant {
            qb.push("AND rentals_listings.tenant = ").push_bind(t.clone()).push(" ");
        }
        if !f.nft_ids.is_empty() {
            qb.push("AND rentals.metadata_id = ANY(")
                .push_bind(f.nft_ids.clone())
                .push(") ");
        }

        qb.push("GROUP BY rentals.id, rentals_listings.id, periods.rental_id ");

        // HAVING (min/max price per day)
        let mut having_started = false;
        if let Some(min) = &f.min_price_per_day {
            qb.push(" HAVING max(periods.price_per_day) >= ")
                .push_bind(min.clone())
                .push("::numeric ");
            having_started = true;
        }
        if let Some(max) = &f.max_price_per_day {
            qb.push(if having_started { " AND " } else { " HAVING " })
                .push("min(periods.price_per_day) <= ")
                .push_bind(max.clone())
                .push("::numeric ");
        }

        qb.push("ORDER BY rentals.metadata_id, rentals.created_at desc) as rentals \
                 WHERE metadata.id = rentals.metadata_id ");

        // metadata filters
        if let Some(c) = &f.category {
            qb.push("AND metadata.category = ").push_bind(c.clone()).push(" ");
        }
        if let Some(text) = &f.text {
            qb.push("AND metadata.search_text ILIKE '%' || ")
                .push_bind(text.clone())
                .push(" || '%' ");
        }
        if let Some(min) = f.min_distance_to_plaza {
            qb.push("AND metadata.distance_to_plaza >= ").push_bind(min).push(" ");
        }
        if let Some(max) = f.max_distance_to_plaza {
            if f.min_distance_to_plaza.is_none() {
                qb.push("AND metadata.distance_to_plaza >= 0 ");
            }
            qb.push("AND metadata.distance_to_plaza <= ").push_bind(max).push(" ");
        }
        if let Some(adj) = f.adjacent_to_road {
            qb.push("AND metadata.adjacent_to_road = ").push_bind(adj).push(" ");
        }
        if let Some(min) = f.min_estate_size {
            if min >= 0 {
                qb.push("AND metadata.estate_size >= ").push_bind(min).push(" ");
            }
        }
        if let Some(max) = f.max_estate_size {
            qb.push("AND metadata.estate_size <= ").push_bind(max).push(" ");
        }

        // ORDER BY
        let dir = match q.sort_direction.as_deref() {
            Some("desc") => "desc",
            _ => "asc",
        };
        let order = match q.sort_by.as_deref() {
            Some("land_creation_date") => format!("ORDER BY metadata.created_at {dir} "),
            Some("name") => format!("ORDER BY metadata.search_text {dir} "),
            Some("max_rental_price") => format!("ORDER BY rentals.max_price_per_day {dir} "),
            Some("min_rental_price") => format!("ORDER BY rentals.min_price_per_day {dir} "),
            // default: rental_listing_date
            _ => format!("ORDER BY rentals.created_at {dir} "),
        };
        qb.push(order);
        qb.push("LIMIT ").push_bind(q.limit).push(" OFFSET ").push_bind(q.offset);

        let rows = qb.build().fetch_all(&self.pool).await?;
        let total: i64 = rows
            .first()
            .map(|r| r.try_get::<i64, _>("rentals_listings_count").unwrap_or(0))
            .unwrap_or(0);
        let results: Vec<RentalListing> = rows.iter().map(row_to_listing).collect();
        let page = if q.limit > 0 { q.offset / q.limit } else { 0 };
        let pages = if q.limit > 0 {
            (total + q.limit - 1) / q.limit
        } else {
            0
        };

        Ok(PaginatedListings {
            results,
            total,
            page,
            pages,
            limit: q.limit,
        })
    }

    /// GET /v1/rental-listings/prices — { "<pricePerDay>": count, ... }
    pub async fn get_prices(
        &self,
        f: &PriceFilters,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "SELECT periods.price_per_day::text as price_per_day, COUNT(DISTINCT rentals.id) as count \
             FROM rentals, rentals_listings, periods, metadata ",
        );
        if !f.rental_days.is_empty() {
            qb.push(", (SELECT DISTINCT rental_id FROM periods WHERE ");
            let mut sep = qb.separated(" OR ");
            for d in &f.rental_days {
                sep.push("(min_days <= ");
                sep.push_bind_unseparated(*d);
                sep.push_unseparated(" AND max_days >= ");
                sep.push_bind_unseparated(*d);
                sep.push_unseparated(")");
            }
            qb.push(") as rental_days_periods ");
        }
        qb.push(
            "WHERE rentals.id = rentals_listings.id AND periods.rental_id = rentals.id \
             AND metadata.id = rentals.metadata_id AND rentals.status = 'open' ",
        );
        if !f.rental_days.is_empty() {
            qb.push("AND rental_days_periods.rental_id = rentals.id ");
        }
        if let Some(c) = &f.category {
            qb.push("AND metadata.category = ").push_bind(c.clone()).push(" ");
        }
        if let Some(adj) = f.adjacent_to_road {
            qb.push("AND metadata.adjacent_to_road = ").push_bind(adj).push(" ");
        }
        if let Some(min) = f.min_distance_to_plaza {
            qb.push("AND metadata.distance_to_plaza >= ").push_bind(min).push(" ");
        }
        if let Some(max) = f.max_distance_to_plaza {
            qb.push("AND metadata.distance_to_plaza <= ").push_bind(max).push(" ");
        }
        if let Some(min) = f.min_estate_size {
            qb.push("AND metadata.estate_size >= ").push_bind(min).push(" ");
        }
        if let Some(max) = f.max_estate_size {
            qb.push("AND metadata.estate_size <= ").push_bind(max).push(" ");
        }
        qb.push("GROUP BY periods.price_per_day");

        let rows = qb.build().fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let p: String = r.try_get("price_per_day").unwrap_or_default();
                let c: i64 = r.try_get("count").unwrap_or(0);
                (p, c)
            })
            .collect())
    }
}

/// Single canonical listing row, identical projection to the refresh return
/// query in component.ts, used by get_listing_by_id and insert re-read.
const LISTING_BY_ID_SQL: &str = r#"
SELECT rentals.*, metadata.category, metadata.search_text, metadata.created_at as metadata_created_at
FROM metadata,
  (SELECT rentals.*, rentals_listings.tenant, rentals_listings.lessor,
     jsonb_agg(jsonb_build_array(periods.min_days::text, periods.max_days::text, periods.price_per_day::text) ORDER BY periods.min_days) as periods
   FROM rentals, rentals_listings, periods
   WHERE rentals.id = rentals_listings.id AND periods.rental_id = rentals.id
   GROUP BY rentals.id, rentals_listings.id) as rentals
WHERE metadata.id = rentals.metadata_id AND rentals.id = $1
"#;

/// fromDBGetRentalsListingsToRentalListings — map a DB row to the API shape.
fn row_to_listing(r: &PgRow) -> RentalListing {
    // `periods` is a jsonb array of `[min, max, price]` string triples.
    let periods_raw: serde_json::Value =
        r.try_get("periods").unwrap_or(serde_json::Value::Null);
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
        id: r.get::<Uuid, _>("id").to_string(),
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
