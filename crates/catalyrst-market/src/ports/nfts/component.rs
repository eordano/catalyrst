use sqlx::PgPool;
use sqlx::Row;

use crate::dcl_schemas::NftCategory;
use crate::http::response::ApiError;

use super::query::{build_nfts_query, Bind};
use super::rows::from_db_nft_to_nft;
use super::types::{DbNft, NftErrors, NftFilters, NftResult};

pub struct NftsComponent {
    pool: PgPool,
    rentals: crate::ports::rentals::RentalsComponent,
}

impl NftsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            rentals: crate::ports::rentals::RentalsComponent::new(None),
        }
    }

    pub fn with_rentals(pool: PgPool, rentals: crate::ports::rentals::RentalsComponent) -> Self {
        Self { pool, rentals }
    }

    pub async fn get_nfts(
        &self,
        filters: &NftFilters,
        _caller: Option<String>,
    ) -> Result<(Vec<NftResult>, i64), ApiError> {
        if filters.owner.is_some() && filters.tenant.is_some() {
            return Err(ApiError::bad_request(
                NftErrors::INVALID_SEARCH_BY_TENANT_AND_OWNER,
            ));
        }
        if let Some(ref tid) = filters.token_id {
            if !tid.chars().all(|c| c.is_ascii_digit()) || tid.is_empty() {
                return Err(ApiError::bad_request(NftErrors::INVALID_TOKEN_ID));
            }
            if filters.contract_addresses.is_empty() {
                return Err(ApiError::bad_request(NftErrors::MISSING_CONTRACT_ADDRESS));
            }
        }

        let wants_rentals = self.rentals.is_enabled()
            && filters.is_on_rent
            && (matches!(
                filters.category,
                Some(NftCategory::Estate) | Some(NftCategory::Parcel)
            ) || filters.is_land);

        let mut effective = filters.clone();
        let mut prefetched_listings: Option<Vec<crate::ports::rentals::RentalListing>> = None;
        if wants_rentals {
            let statuses = if effective.rental_status.is_empty() {
                vec!["open".to_string()]
            } else {
                effective.rental_status.clone()
            };

            let listings = self.rentals.get_open_rentals(&statuses).await;
            effective.ids = listings.iter().map(|l| l.nft_id.clone()).collect();
            prefetched_listings = Some(listings);

            if effective.ids.is_empty() {
                return Ok((Vec::new(), 0));
            }
        }

        let (sql, binds) = build_nfts_query(&effective, false);
        let mut q = sqlx::query_as::<_, DbNft>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = match b {
                Bind::Text(s) => q.bind(s.clone()),
                Bind::TextArray(v) => q.bind(v.clone()),
                Bind::Int(i) => q.bind(*i),
                Bind::Float(f) => q.bind(*f),
            };
        }
        let rows: Vec<DbNft> = q.fetch_all(&self.pool).await?;

        let (count_sql, count_binds) = build_nfts_query(&effective, true);
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        for b in &count_binds {
            cq = match b {
                Bind::Text(s) => cq.bind(s.clone()),
                Bind::TextArray(v) => cq.bind(v.clone()),
                Bind::Int(i) => cq.bind(*i),
                Bind::Float(f) => cq.bind(*f),
            };
        }
        let total: i64 = cq.fetch_one(&self.pool).await?;

        let nft_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let orders_by_nft = if nft_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            self.get_open_orders_by_nft_ids(&nft_ids, effective.owner.as_deref())
                .await?
        };

        let listings: Vec<crate::ports::rentals::RentalListing> =
            if let Some(l) = prefetched_listings {
                l
            } else if self.rentals.is_enabled() {
                let land_ids: Vec<String> = rows
                    .iter()
                    .filter(|r| matches!(r.category.as_deref(), Some("parcel") | Some("estate")))
                    .map(|r| r.id.clone())
                    .collect();
                if land_ids.is_empty() {
                    Vec::new()
                } else {
                    let statuses = if filters.rental_status.is_empty() {
                        vec!["open".to_string()]
                    } else {
                        filters.rental_status.clone()
                    };
                    self.rentals
                        .get_rentals_listings_of_nfts(&land_ids, &statuses)
                        .await
                }
            } else {
                Vec::new()
            };
        let listing_by_nft: std::collections::HashMap<&str, &crate::ports::rentals::RentalListing> =
            listings.iter().map(|l| (l.nft_id.as_str(), l)).collect();

        let results = rows
            .iter()
            .map(|r| {
                let order = orders_by_nft.get(&r.id);
                let listing = listing_by_nft.get(r.id.as_str()).copied();
                let mut nft = from_db_nft_to_nft(r);
                nft.active_order_id = order.map(|o| o.id.clone());
                nft.open_rental_id = listing.map(|l| l.id.clone());
                NftResult {
                    nft,
                    order: order
                        .map(|o| serde_json::to_value(o).unwrap_or(serde_json::Value::Null)),
                    rental: listing
                        .map(|l| serde_json::to_value(l).unwrap_or(serde_json::Value::Null)),
                }
            })
            .collect();
        Ok((results, total))
    }

    pub async fn rental_assets_ids_for_owner(&self, owner: &str) -> Vec<String> {
        self.rentals.get_rental_assets_ids_for_lessor(owner).await
    }

    async fn get_open_orders_by_nft_ids(
        &self,
        nft_ids: &[String],
        owner: Option<&str>,
    ) -> Result<std::collections::HashMap<String, crate::ports::orders::Order>, ApiError> {
        let sql = crate::ports::orders::build_open_orders_by_nft_ids_sql(owner.is_some());

        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(nft_ids);
        if let Some(o) = owner {
            q = q.bind(o.to_string());
        }
        let db_rows = q.fetch_all(&self.pool).await?;

        let mut map = std::collections::HashMap::new();
        for row in &db_rows {
            let nft_id: String = row.try_get("nft_id").unwrap_or_default();
            map.entry(nft_id)
                .or_insert_with(|| crate::ports::orders::row_to_order(row));
        }
        Ok(map)
    }
}
