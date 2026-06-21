//! Local Subsquid marketplace DB access for the rentals cross-checks.
//!
//! Upstream `signatures-server` resolved NFT ownership + metadata and on-chain
//! rental state from two TheGraph subgraphs (`MARKETPLACE_SUBGRAPH_URL`,
//! `RENTALS_SUBGRAPH_URL`). This deploy has neither subgraph but DOES
//! index the marketplace into a local Subsquid DB, so we read NFT ownership +
//! metadata straight from the marketplace `nft` table via sqlx — the same data
//! the marketplace subgraph would have returned.
//!
//! What is and isn't available locally:
//!   * NFT existence / owner / category / search_* metadata — YES, `nft` table.
//!   * On-chain rental state (the rentals subgraph `rentals` entity, used by
//!     upstream's "is there an open on-chain rental" check) — NO, not mirrored
//!     into squid. That check therefore degrades to the DB unique-open-rental
//!     constraint (`rentals_token_id_contract_address_status_unique_index`),
//!     which catches every listing this server itself created. See
//!     `handlers::create_rentals_listing`.

use sqlx::{PgPool, Row};

/// The marketplace-NFT facts the rentals create/refresh paths need. Mirrors the
/// subset of the subgraph `NFT` type that `createRentalListing` /
/// `refreshRentalListing` consume.
#[derive(Debug, Clone)]
pub struct SquidNft {
    /// `<contract>-<tokenId>` — the metadata table key (matches upstream's use
    /// of `nft.id` for the rentals `metadata` row).
    pub metadata_id: String,
    /// NFT category, e.g. "parcel" | "estate".
    pub category: String,
    /// Lowercased current owner address.
    pub owner_address: String,
    pub search_text: String,
    pub distance_to_plaza: Option<i32>,
    pub adjacent_to_road: Option<bool>,
    pub estate_size: Option<i32>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct SquidMarketplace {
    pool: PgPool,
    schema: String,
}

impl SquidMarketplace {
    pub fn new(pool: PgPool, schema: String) -> Self {
        Self { pool, schema }
    }

    /// Look up a single NFT by contract address + token id. `token_id` is the
    /// decimal string the rentals payload carries; the squid column is numeric.
    /// Returns `None` if the NFT is not indexed.
    pub async fn nft_by_contract_token(
        &self,
        contract_address: &str,
        token_id: &str,
    ) -> Result<Option<SquidNft>, sqlx::Error> {
        // Schema name is operator config, not user input; interpolate (sqlx
        // can't bind identifiers). contract/token are bound parameters.
        // Numeric columns are cast to bigint in SQL so we can read them as i64
        // without pulling in a decimal crate. contract/token are bound params;
        // the schema identifier is operator config (interpolated, not user input).
        let sql = format!(
            "SELECT category, owner_address, COALESCE(search_text, '') AS search_text, \
                    search_distance_to_plaza, search_adjacent_to_road, search_estate_size, \
                    COALESCE(created_at, 0)::bigint AS created_at, \
                    COALESCE(updated_at, 0)::bigint AS updated_at \
             FROM {schema}.nft \
             WHERE lower(contract_address) = lower($1) AND token_id = $2::numeric \
             LIMIT 1",
            schema = self.schema
        );
        let row = sqlx::query(&sql)
            .bind(contract_address)
            .bind(token_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| {
            let owner: Option<String> = r.try_get("owner_address").ok();
            SquidNft {
                metadata_id: format!("{}-{}", contract_address.to_lowercase(), token_id),
                category: r.try_get("category").unwrap_or_default(),
                owner_address: owner.unwrap_or_default().to_lowercase(),
                search_text: r.try_get("search_text").unwrap_or_default(),
                distance_to_plaza: r.try_get("search_distance_to_plaza").ok(),
                adjacent_to_road: r.try_get("search_adjacent_to_road").ok(),
                estate_size: r.try_get("search_estate_size").ok(),
                created_at: r.try_get("created_at").unwrap_or(0),
                updated_at: r.try_get("updated_at").unwrap_or(0),
            }
        }))
    }
}
