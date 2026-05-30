//! Request/response shapes, mirroring @dcl/schemas RentalListing,
//! RentalListingCreation and RentalListingPeriod.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RentalListingPeriodInput {
    #[serde(rename = "minDays")]
    pub min_days: i64,
    #[serde(rename = "maxDays")]
    pub max_days: i64,
    #[serde(rename = "pricePerDay")]
    pub price_per_day: String,
}

/// POST /v1/rentals-listings body — @dcl/schemas RentalListingCreation.
#[derive(Debug, Clone, Deserialize)]
pub struct RentalListingCreation {
    pub network: String,
    #[serde(rename = "chainId")]
    pub chain_id: i64,
    /// epoch milliseconds
    pub expiration: i64,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    /// [contractNonce, signerNonce, assetNonce]
    pub nonces: Vec<String>,
    pub periods: Vec<RentalListingPeriodInput>,
    #[serde(rename = "rentalContractAddress")]
    pub rental_contract_address: String,
    pub signature: String,
    #[serde(default = "address_zero")]
    pub target: String,
}

fn address_zero() -> String {
    "0x0000000000000000000000000000000000000000".to_string()
}

/// Response period.
#[derive(Debug, Clone, Serialize)]
pub struct RentalListingPeriod {
    #[serde(rename = "minDays")]
    pub min_days: i64,
    #[serde(rename = "maxDays")]
    pub max_days: i64,
    #[serde(rename = "pricePerDay")]
    pub price_per_day: String,
}

/// Response object — @dcl/schemas RentalListing.
#[derive(Debug, Clone, Serialize)]
pub struct RentalListing {
    pub id: String,
    #[serde(rename = "nftId")]
    pub nft_id: String,
    pub category: String,
    #[serde(rename = "searchText")]
    pub search_text: String,
    pub network: String,
    #[serde(rename = "chainId")]
    pub chain_id: i64,
    /// epoch milliseconds
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
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(rename = "startedAt")]
    pub started_at: Option<i64>,
    pub periods: Vec<RentalListingPeriod>,
    pub target: String,
    #[serde(rename = "rentedDays")]
    pub rented_days: Option<i64>,
}

/// Paginated GET envelope `data`.
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedListings {
    pub results: Vec<RentalListing>,
    pub total: i64,
    pub page: i64,
    pub pages: i64,
    pub limit: i64,
}

/// The contract-domain projection used for EIP-712 signature verification.
/// Mirrors logic/rentals/types.ts ContractRentalListing.
#[derive(Debug, Clone)]
pub struct ContractRentalListing {
    pub signer: String,
    pub contract_address: String,
    pub token_id: String,
    /// expiration in **seconds** since epoch (string)
    pub expiration: String,
    pub indexes: Vec<String>,
    pub price_per_day: Vec<String>,
    pub max_days: Vec<String>,
    pub min_days: Vec<String>,
    pub signature: String,
    pub target: String,
}

impl ContractRentalListing {
    /// fromRentalCreationToContractRentalListing(lessor, rental)
    pub fn from_creation(lessor: &str, r: &RentalListingCreation) -> Self {
        Self {
            signer: lessor.to_string(),
            contract_address: r.contract_address.clone(),
            token_id: r.token_id.clone(),
            expiration: (r.expiration / 1000).to_string(),
            indexes: r.nonces.clone(),
            price_per_day: r.periods.iter().map(|p| p.price_per_day.clone()).collect(),
            max_days: r.periods.iter().map(|p| p.max_days.to_string()).collect(),
            min_days: r.periods.iter().map(|p| p.min_days.to_string()).collect(),
            signature: r.signature.clone(),
            target: r.target.clone(),
        }
    }
}
