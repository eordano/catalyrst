use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::json;
use std::collections::HashMap;

use crate::db::{ListingFilters, ListingQuery, PriceFilters};
use crate::http::{ApiError, Ok2};
use crate::signature::{has_valid_v, verify_rentals_listing_signature};
use crate::types::{ContractRentalListing, RentalListingCreation};
use crate::AppState;

const ADDRESS_ZERO: &str = "0x0000000000000000000000000000000000000000";
const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 100;

fn pagination(params: &HashMap<String, String>) -> (i64, i64) {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .map(|l| l.clamp(1, MAX_LIMIT))
        .unwrap_or(DEFAULT_LIMIT);
    let offset = params
        .get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|o| *o >= 0)
        .unwrap_or(0);
    (limit, offset)
}

fn all_values(raw_query: &str, key: &str) -> Vec<String> {
    raw_query
        .split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            if k != key {
                return None;
            }
            let v = it.next().unwrap_or("");
            Some(urldecode(v))
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn urldecode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

pub async fn get_rentals_listings(
    State(state): State<AppState>,
    axum::extract::RawQuery(raw): axum::extract::RawQuery,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let raw = raw.unwrap_or_default();
    let (limit, offset) = pagination(&params);

    let num = |k: &str| -> Result<Option<i32>, ApiError> {
        match params.get(k) {
            Some(v) => v
                .parse::<i32>()
                .map(Some)
                .map_err(|_| ApiError::bad_request(format!("{} must be a number", k))),
            None => Ok(None),
        }
    };
    let boolean = |k: &str| -> Result<Option<bool>, ApiError> {
        match params.get(k) {
            Some(v) if v == "true" => Ok(Some(true)),
            Some(v) if v == "false" => Ok(Some(false)),
            Some(_) => Err(ApiError::bad_request(format!("{} must be a boolean", k))),
            None => Ok(None),
        }
    };

    let filter = ListingFilters {
        category: params.get("category").cloned(),
        text: params.get("text").cloned(),
        lessor: params.get("lessor").cloned(),
        tenant: params.get("tenant").cloned(),
        status: all_values(&raw, "status"),
        token_id: params.get("tokenId").cloned(),
        contract_addresses: all_values(&raw, "contractAddresses"),
        nft_ids: all_values(&raw, "nftIds"),
        network: params.get("network").cloned(),
        updated_after: params.get("updatedAfter").and_then(|s| s.parse().ok()),
        target: Some(
            params
                .get("target")
                .cloned()
                .unwrap_or_else(|| ADDRESS_ZERO.to_string()),
        ),
        min_price_per_day: params.get("minPricePerDay").cloned(),
        max_price_per_day: params.get("maxPricePerDay").cloned(),
        min_distance_to_plaza: num("minDistanceToPlaza")?,
        max_distance_to_plaza: num("maxDistanceToPlaza")?,
        min_estate_size: num("minEstateSize")?,
        max_estate_size: num("maxEstateSize")?,
        adjacent_to_road: boolean("adjacentToRoad")?,
        rental_days: all_values(&raw, "rentalDays")
            .iter()
            .filter_map(|s| s.parse::<i32>().ok())
            .filter(|d| *d != 0)
            .collect(),
    };

    let query = ListingQuery {
        sort_by: params.get("sortBy").cloned(),
        sort_direction: params.get("sortDirection").cloned(),
        offset,
        limit,
        filter,
        history: params.get("history").map(|s| s == "true").unwrap_or(false),
    };

    let page = state.db.get_listings(&query).await?;
    Ok(Ok2(StatusCode::OK, page))
}

pub async fn create_rentals_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Json(body): axum::extract::Json<RentalListingCreation>,
) -> Result<axum::response::Response, axum::response::Response> {
    use axum::response::IntoResponse;

    let signer = catalyrst_crypto_require_signer(&state, &headers, "post", "/v1/rentals-listings")
        .map_err(|e| ApiError::Unauthorized(e).into_response())?;

    let now_ms = chrono::Utc::now().timestamp_millis();
    if body.expiration < now_ms {
        return Err(
            ApiError::BadRequest("The rental listing has expired".to_string())
                .with_data(json!({
                    "contractAddress": body.contract_address,
                    "tokenId": body.token_id,
                    "expiration": body.expiration,
                }))
                .into_response(),
        );
    }

    let contract = ContractRentalListing::from_creation(&signer, &body);
    let chain_id = body.chain_id as u64;
    match verify_rentals_listing_signature(&contract, chain_id) {
        Ok(true) => {}
        Ok(false) => {
            let msg = if !has_valid_v(&body.signature) {
                "The server does not accept ECDSA signatures with V as 0 or 1"
            } else {
                "The provided signature is invalid"
            };
            return Err(ApiError::BadRequest(msg.to_string()).into_response());
        }
        Err(e) => {
            return Err(ApiError::BadRequest(e.to_string()).into_response());
        }
    }

    let (
        nft_id,
        category,
        search_text,
        distance_to_plaza,
        adjacent_to_road,
        estate_size,
        created_at,
        updated_at,
    ) = if let Some(squid) = &state.squid {
        let nft = squid
            .nft_by_contract_token(&body.contract_address, &body.token_id)
            .await
            .map_err(|e| ApiError::from(e).into_response())?;
        let nft = nft.ok_or_else(|| {
            ApiError::not_found("NFT not found")
                .with_data(json!({
                    "contractAddress": body.contract_address,
                    "tokenId": body.token_id,
                }))
                .into_response()
        })?;

        if !nft.owner_address.eq_ignore_ascii_case(&signer) {
            return Err(ApiError::Unauthorized(format!(
                "The owner of the NFT {} is not the lessor {}",
                nft.owner_address, signer
            ))
            .into_response());
        }

        if nft.category.eq_ignore_ascii_case("estate") && nft.estate_size.unwrap_or(0) == 0 {
            return Err(ApiError::BadRequest(
                "The provided Estate does not have any parcels".to_string(),
            )
            .into_response());
        }

        let created = chrono::DateTime::<chrono::Utc>::from_timestamp(nft.created_at, 0)
            .map(|d| d.naive_utc())
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());
        let updated = chrono::DateTime::<chrono::Utc>::from_timestamp(nft.updated_at, 0)
            .map(|d| d.naive_utc())
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());
        (
            nft.metadata_id,
            nft.category,
            nft.search_text,
            nft.distance_to_plaza,
            nft.adjacent_to_road,
            nft.estate_size,
            created,
            updated,
        )
    } else {
        let now = chrono::Utc::now().naive_utc();
        (
            format!("{}-{}", body.contract_address, body.token_id),
            "parcel".to_string(),
            String::new(),
            None,
            None,
            None,
            now,
            now,
        )
    };

    let inserted = state
        .db
        .insert_listing(
            &nft_id,
            &category,
            &search_text,
            distance_to_plaza,
            adjacent_to_road,
            estate_size,
            created_at,
            updated_at,
            &body,
            &signer,
        )
        .await;

    match inserted {
        Ok(listing) => Ok(Ok2(StatusCode::CREATED, listing).into_response()),
        Err(e) if crate::db::Database::is_open_conflict(&e) => Err(ApiError::Conflict(
            "There is already an open rental listing for the asset".to_string(),
        )
        .with_data(json!({
            "contractAddress": body.contract_address,
            "tokenId": body.token_id,
        }))
        .into_response()),
        Err(e) => Err(ApiError::from(e).into_response()),
    }
}

pub async fn refresh_rentals_listing(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let listing = state
        .db
        .get_listing_by_id(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("Rental listing was not found"))?;

    let Some(squid) = &state.squid else {
        return Ok(Ok2(StatusCode::OK, listing));
    };

    match squid
        .nft_by_contract_token(&listing.contract_address, &listing.token_id)
        .await?
    {
        Some(nft) => {
            let updated = chrono::DateTime::<chrono::Utc>::from_timestamp(nft.updated_at, 0)
                .map(|d| d.naive_utc())
                .unwrap_or_else(|| chrono::Utc::now().naive_utc());
            state
                .db
                .update_metadata_for_rental(
                    &id,
                    &nft.category,
                    &nft.search_text,
                    nft.distance_to_plaza,
                    nft.adjacent_to_road,
                    nft.estate_size,
                    updated,
                )
                .await?;
        }
        None => {
            tracing::debug!(rental_id = %id, "NFT not found in squid during refresh; metadata unchanged");
        }
    }

    let refreshed = state
        .db
        .get_listing_by_id(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("Rental listing was not found"))?;
    Ok(Ok2(StatusCode::OK, refreshed))
}

pub async fn get_rental_listings_prices(
    State(state): State<AppState>,
    axum::extract::RawQuery(raw): axum::extract::RawQuery,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let raw = raw.unwrap_or_default();
    let num = |k: &str| -> Result<Option<i32>, ApiError> {
        match params.get(k) {
            Some(v) => v
                .parse::<i32>()
                .map(Some)
                .map_err(|_| ApiError::bad_request(format!("{} must be a number", k))),
            None => Ok(None),
        }
    };
    let boolean = |k: &str| -> Option<bool> {
        match params.get(k).map(|s| s.as_str()) {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        }
    };
    let filters = PriceFilters {
        category: params.get("category").cloned(),
        adjacent_to_road: boolean("adjacentToRoad"),
        min_distance_to_plaza: num("minDistanceToPlaza")?,
        max_distance_to_plaza: num("maxDistanceToPlaza")?,
        min_estate_size: num("minEstateSize")?,
        max_estate_size: num("maxEstateSize")?,
        rental_days: all_values(&raw, "rentalDays")
            .iter()
            .filter_map(|s| s.parse::<i32>().ok())
            .filter(|d| *d != 0)
            .collect(),
    };

    let prices = state.db.get_prices(&filters).await?;
    let map: serde_json::Map<String, serde_json::Value> =
        prices.into_iter().map(|(p, c)| (p, json!(c))).collect();
    Ok(Ok2(StatusCode::OK, serde_json::Value::Object(map)))
}

fn catalyrst_crypto_require_signer(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, String> {
    crate::auth::require_signer(headers, method, path, state.config.auth_expiration_secs)
        .map_err(|e| e.to_string())
}
