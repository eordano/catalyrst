use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use serde_json::{json, Value};

use crate::errors::{bad_request, service_unavailable};
use crate::query_params::{
    is_valid_eth_address, parse_pagination_with, parse_query_string, NonPositivePolicy,
    OversizePolicy, MAX_PAGE_SIZE,
};
use crate::state::AppState;

fn land_source_unavailable(detail: &str) -> Response {
    service_unavailable(&format!(
        "LAND rights are unavailable on this node: {detail}"
    ))
}

fn external_land_error(route: &'static str, eth_network: &str, error: &str) -> Response {
    use crate::handlers::external_graph;
    if let Err(missing) = external_graph::subgraph_urls(eth_network) {
        tracing::warn!(route, error = %missing, "land route has no data source: land_authz index absent and subgraph env unset");
        return land_source_unavailable(
            "no local land_authz index and no LAND subgraph is configured",
        );
    }
    tracing::error!(route, error = %error, "external land subgraph query failed");
    crate::errors::internal_server_error()
}

fn parcel_or_estate_not_found(x: i64, y: i64) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiErrorBody::labeled(
            "Bad Request",
            format!("Parcel or estate rights not found for x: {x}, y: {y}"),
        )),
    )
        .into_response()
}

const PARCEL_MIN: i64 = -150;
const PARCEL_MAX: i64 = 150;

fn empty_page() -> Value {
    json!({"elements": [], "totalAmount": 0, "pageNum": 1, "pageSize": 100})
}

fn parse_pagination(req: &Request, max_page_size: i64) -> (i64, i64) {
    let params = parse_query_string(req.uri().query().unwrap_or(""));

    let p = parse_pagination_with(
        &params,
        max_page_size,
        OversizePolicy::Clamp,
        NonPositivePolicy::ClampToOne,
    )
    .unwrap_or(crate::query_params::Pagination {
        page_size: 1,
        page_num: 1,
        offset: 0,
        limit: 1,
    });
    (p.page_size, p.page_num)
}

fn page_offset(page_num: i64, page_size: i64) -> i64 {
    page_num.saturating_sub(1).saturating_mul(page_size)
}

pub async fn user_names(
    State(state): State<Arc<AppState>>,
    Path(addr): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let (page_size, page_num) = parse_pagination(&req, MAX_PAGE_SIZE as i64);

    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => return Json(empty_page()),
    };

    let owner = addr.to_lowercase();

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM squid_marketplace.nft \
         WHERE category = 'ens' AND owner_address = lower($1)",
    )
    .bind(&owner)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let offset = page_offset(page_num, page_size);

    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT n.name, n.contract_address, n.token_id::text, o.price::text \
         FROM squid_marketplace.nft n \
         LEFT JOIN squid_marketplace.\"order\" o ON o.id = n.active_order_id \
         WHERE n.category = 'ens' AND n.owner_address = lower($1) \
         ORDER BY n.id ASC \
         LIMIT $2 OFFSET $3",
    )
    .bind(&owner)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let elements: Vec<Value> = rows
        .into_iter()
        .map(|(name, contract_address, token_id, price)| {
            let mut obj = json!({
                "name": name,
                "contractAddress": contract_address,
                "tokenId": token_id,
            });

            if let Some(p) = price {
                obj["price"] = json!(p);
            }
            obj
        })
        .collect();

    Json(json!({
        "elements": elements,
        "totalAmount": total,
        "pageNum": page_num,
        "pageSize": page_size,
    }))
}

pub async fn user_lands(
    State(state): State<Arc<AppState>>,
    Path(addr): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let (page_size, page_num) = parse_pagination(&req, MAX_PAGE_SIZE as i64);

    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,
        None => return Json(empty_page()),
    };

    let owner = addr.to_lowercase();

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM squid_marketplace.nft \
         WHERE category IN ('parcel','estate') AND owner_address = lower($1)",
    )
    .bind(&owner)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let (limit, offset) = (page_size, page_offset(page_num, page_size));

    let rows: Vec<(
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT \
            n.name, \
            n.contract_address, \
            n.token_id::text, \
            n.category::text, \
            n.search_parcel_x::text, \
            n.search_parcel_y::text, \
            COALESCE(pd.description, ed.description) AS description, \
            (pd.id IS NOT NULL OR ed.id IS NOT NULL) AS has_data, \
            o.price::text AS price, \
            n.image \
         FROM squid_marketplace.nft n \
         LEFT JOIN squid_marketplace.parcel p ON p.id = n.parcel_id \
         LEFT JOIN squid_marketplace.data pd ON pd.id = p.data_id \
         LEFT JOIN squid_marketplace.estate e ON e.id = n.estate_id \
         LEFT JOIN squid_marketplace.data ed ON ed.id = e.data_id \
         LEFT JOIN squid_marketplace.\"order\" o ON o.id = n.active_order_id \
         WHERE n.category IN ('parcel','estate') AND n.owner_address = lower($1) \
         ORDER BY n.transferred_at DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(&owner)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let land_base = state.land_image_base_url.trim_end_matches('/').to_string();
    let rewrite_image = move |img: String| -> String {
        match img.strip_prefix("https://api.decentraland.org") {
            Some(rest) => format!("{land_base}{rest}"),
            None => img,
        }
    };

    let elements: Vec<Value> = rows
        .into_iter()
        .map(
            |(
                name,
                contract_address,
                token_id,
                category,
                x,
                y,
                description,
                has_data,
                price,
                image,
            )| {
                let is_parcel = category == "parcel";
                let mut obj = json!({
                    "contractAddress": contract_address,
                    "tokenId": token_id,
                    "category": category,
                });
                if let Some(n) = name {
                    obj["name"] = json!(n);
                }
                if is_parcel {
                    if let Some(xv) = x {
                        obj["x"] = json!(xv);
                    }
                    if let Some(yv) = y {
                        obj["y"] = json!(yv);
                    }
                }

                if has_data {
                    obj["description"] = json!(description.unwrap_or_default());
                }
                if let Some(p) = price {
                    obj["price"] = json!(p);
                }
                if let Some(img) = image {
                    obj["image"] = json!(rewrite_image(img));
                }
                obj
            },
        )
        .collect();

    Json(json!({
        "elements": elements,
        "totalAmount": total,
        "pageNum": page_num,
        "pageSize": page_size,
    }))
}

/// `None` when there is no local index to answer from, which is the only case
/// that still warrants a remote round trip.
async fn local_parcels_by_update_operator(
    state: &AppState,
    update_operator: &str,
) -> Option<Result<Vec<Value>, sqlx::Error>> {
    let pool = state.squid_pool.as_ref()?;
    if !crate::land_operators::local_index_present(pool).await {
        return None;
    }
    let store = catalyrst_land_authz::LandAuthzStore::new(pool.clone());
    Some(
        store
            .parcels_with_update_operator(update_operator)
            .await
            .map(|parcels| {
                parcels
                    .into_iter()
                    .map(|p| {
                        json!({
                            "id": p.token_id,
                            "x": p.x.to_string(),
                            "y": p.y.to_string(),
                            "owner": p.owner,
                            "updateOperator": update_operator,
                        })
                    })
                    .collect()
            }),
    )
}

pub async fn user_lands_permissions(
    State(state): State<Arc<AppState>>,
    Path(addr): Path<String>,
    req: Request,
) -> Response {
    use crate::handlers::external_graph;

    let update_operator = addr.to_lowercase();

    let elements = match local_parcels_by_update_operator(&state, &update_operator).await {
        Some(Ok(e)) => e,
        Some(Err(e)) => {
            tracing::error!(update_operator = %update_operator, error = %e, "local lands-permissions lookup failed");
            return crate::errors::internal_server_error();
        }
        None => {
            match external_graph::parcels_by_update_operator(&state.eth_network, &update_operator)
                .await
            {
                Ok(e) => e,
                Err(e) => return external_land_error("lands-permissions", &state.eth_network, &e),
            }
        }
    };

    let (page_size, page_num) = parse_pagination(&req, MAX_PAGE_SIZE as i64);

    let total = elements.len() as i64;
    let off = page_offset(page_num, page_size);
    let start = off.max(0).min(total) as usize;
    let end = off.saturating_add(page_size).max(0).min(total) as usize;
    let page: Vec<Value> = if start < end {
        elements[start..end].to_vec()
    } else {
        Vec::new()
    };

    Json(json!({
        "elements": page,
        "totalAmount": total,
        "pageNum": page_num,
        "pageSize": page_size,
    }))
    .into_response()
}

const COORDS_ERR: &str = "Coordinates X and Y must be valid numbers in a valid range";

fn validate_coords(x: &str, y: &str) -> Result<(i64, i64), Response> {
    let xi: i64 = x.parse().map_err(|_| bad_request(COORDS_ERR))?;
    let yi: i64 = y.parse().map_err(|_| bad_request(COORDS_ERR))?;
    if !(PARCEL_MIN..=PARCEL_MAX).contains(&xi) || !(PARCEL_MIN..=PARCEL_MAX).contains(&yi) {
        return Err(bad_request(COORDS_ERR));
    }
    Ok((xi, yi))
}

pub async fn parcel_operators(
    State(state): State<Arc<AppState>>,
    Path((x, y)): Path<(String, String)>,
) -> Response {
    use crate::handlers::external_graph;

    let (xi, yi) = match validate_coords(&x, &y) {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Some(pool) = state.squid_pool.as_ref() {
        if crate::land_operators::local_index_present(pool).await {
            let store = catalyrst_land_authz::LandAuthzStore::new(pool.clone());
            return match store.parcel_subject(xi as i32, yi as i32).await {
                Ok(Some(subject)) => {
                    let managers = store
                        .account_grants(&subject.registry, &subject.owner, "update_manager")
                        .await
                        .unwrap_or_default();
                    let approved = store
                        .account_grants(&subject.registry, &subject.owner, "approved_for_all")
                        .await
                        .unwrap_or_default();
                    Json(json!({
                        "owner": subject.owner,
                        "operator": subject.operator,
                        "updateOperator": subject.update_operator,
                        "updateManagers": managers,
                        "approvedForAll": approved,
                    }))
                    .into_response()
                }
                Ok(None) => parcel_or_estate_not_found(xi, yi),
                Err(e) => {
                    tracing::warn!(x = xi, y = yi, error = %e, "local parcel operators lookup failed");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            };
        }
    }

    let ops = match external_graph::parcel_operators(&state.eth_network, xi, yi).await {
        Ok(Some(o)) => o,
        Ok(None) => return parcel_or_estate_not_found(xi, yi),
        Err(e) => return external_land_error("parcel-operators", &state.eth_network, &e),
    };

    Json(json!({
        "owner": ops.owner,
        "operator": ops.operator,
        "updateOperator": ops.update_operator,
        "updateManagers": ops.update_managers,
        "approvedForAll": ops.approved_for_all,
    }))
    .into_response()
}

/// Answers from `catalyrst_validator::squid_checker::parcel_permission_flags`,
/// the same call the deploy predicate makes, so this route cannot report a
/// right the validator would then refuse to honour.
pub async fn parcel_permissions(
    State(state): State<Arc<AppState>>,
    Path((address, x, y)): Path<(String, String, String)>,
) -> Response {
    let (xi, yi) = match validate_coords(&x, &y) {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let addr = address.to_lowercase();
    if !is_valid_eth_address(&addr) {
        return bad_request("Address must be a valid Ethereum address");
    }

    let Some(pool) = state.squid_pool.as_ref() else {
        return land_source_unavailable("no marketplace index is configured");
    };

    let resolver = crate::land_operators::resolver_for(pool, &state.eth_network).await;
    match catalyrst_validator::squid_checker::parcel_permission_flags(
        pool,
        Some(resolver.as_ref()),
        &addr,
        xi as i32,
        yi as i32,
    )
    .await
    {
        Ok(Some(flags)) => Json(flags).into_response(),
        Ok(None) => parcel_or_estate_not_found(xi, yi),
        Err(e) => {
            tracing::warn!(x = xi, y = yi, error = %e, "parcel permissions lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// One footprint's worth of `"x,y"` parcels; a request above this answers 400,
/// never a truncated batch.
const MAX_BATCH_PARCELS: usize = 100;

#[derive(Debug, serde::Deserialize)]
pub struct BatchParcelPermissionsRequest {
    pub parcels: Vec<String>,
}

/// The batch's per-entry rule is exactly `validate_coords` (integer parse plus
/// the +/-150 range); any invalid entry rejects the whole request, naming the
/// entry. Duplicates collapse silently to their first occurrence, so the
/// response stays one element per distinct parcel in request order.
fn parse_batch_parcels(entries: &[String]) -> Result<Vec<(i64, i64)>, String> {
    if entries.len() > MAX_BATCH_PARCELS {
        return Err(format!(
            "Too many parcels: {} requested, the limit is {MAX_BATCH_PARCELS}",
            entries.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut parcels = Vec::with_capacity(entries.len());
    for entry in entries {
        let coords = entry
            .split_once(',')
            .ok_or(())
            .and_then(|(x, y)| validate_coords(x, y).map_err(|_| ()));
        let Ok((xi, yi)) = coords else {
            return Err(format!("Invalid parcel \"{entry}\": {COORDS_ERR}"));
        };
        if seen.insert((xi, yi)) {
            parcels.push((xi, yi));
        }
    }
    Ok(parcels)
}

/// Batch analogue of `parcel_permissions`: the five-leg flags for a whole
/// footprint in one response, answered from
/// `catalyrst_validator::squid_checker::parcel_permission_flags_batch` -- the
/// same validator composition the deploy predicate shares, with ownership for
/// the whole set resolved in one squid round trip. A parcel with no index row
/// answers `"permissions": null`, the batch reading of the single route's 404.
pub async fn parcels_permissions_batch(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    Json(body): Json<BatchParcelPermissionsRequest>,
) -> Response {
    let addr = address.to_lowercase();
    if !is_valid_eth_address(&addr) {
        return bad_request("Address must be a valid Ethereum address");
    }

    let parcels = match parse_batch_parcels(&body.parcels) {
        Ok(p) => p,
        Err(msg) => return bad_request(&msg),
    };

    let Some(pool) = state.squid_pool.as_ref() else {
        return land_source_unavailable("no marketplace index is configured");
    };

    let coords: Vec<(i32, i32)> = parcels.iter().map(|&(x, y)| (x as i32, y as i32)).collect();
    let resolver = crate::land_operators::resolver_for(pool, &state.eth_network).await;
    let flags = match catalyrst_validator::squid_checker::parcel_permission_flags_batch(
        pool,
        Some(resolver.as_ref()),
        &addr,
        &coords,
    )
    .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(parcels = coords.len(), error = %e, "batch parcel permissions lookup failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let elements: Vec<Value> = coords
        .iter()
        .zip(flags)
        .map(|(&(x, y), f)| json!({ "x": x, "y": y, "permissions": f }))
        .collect();

    Json(json!({ "elements": elements })).into_response()
}

pub async fn name_owner(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> Response {
    let dcl_name = match name.strip_suffix(".dcl.eth") {
        Some(stripped) => stripped.to_string(),
        None => name.clone(),
    };

    let pool = match state.squid_pool.as_ref() {
        Some(p) => p,

        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let owner: Option<String> = sqlx::query_scalar(
        "SELECT owner_address FROM squid_marketplace.nft \
         WHERE category = 'ens' AND lower(name) = lower($1) \
         ORDER BY id ASC \
         LIMIT 1",
    )
    .bind(&dcl_name)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match owner {
        Some(o) if !o.is_empty() => Json(json!({ "owner": o })).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_text(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("response body");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn batch_parcels_dedupe_silently_in_request_order() {
        let entries: Vec<String> = ["-3,12", "0,0", "-3,12", "150,-150", "0,0"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            parse_batch_parcels(&entries).unwrap(),
            vec![(-3, 12), (0, 0), (150, -150)],
            "duplicates collapse to first occurrence, order preserved"
        );
    }

    #[test]
    fn batch_parcels_reject_any_invalid_entry_naming_it() {
        for bad in [
            "moon", "1;2", "1,2,3", "1,", ",2", "1.5,2", "151,0", "0,-151",
        ] {
            let entries = vec!["0,0".to_string(), bad.to_string()];
            let err = parse_batch_parcels(&entries)
                .expect_err(&format!("entry {bad:?} must reject the whole request"));
            assert!(
                err.contains(&format!("\"{bad}\"")),
                "the error must name the offending entry: {err}"
            );
        }
    }

    #[test]
    fn batch_parcels_capped_at_100() {
        let just_right: Vec<String> = (0..100)
            .map(|i| format!("{},{}", i % 151, i / 151))
            .collect();
        assert_eq!(parse_batch_parcels(&just_right).unwrap().len(), 100);

        let too_many: Vec<String> = (0..101)
            .map(|i| format!("{},{}", i % 151, i / 151))
            .collect();
        let err = parse_batch_parcels(&too_many).expect_err("101 parcels must reject");
        assert!(
            err.contains("101") && err.contains("100"),
            "cap error: {err}"
        );
    }

    #[tokio::test]
    async fn batch_route_rejects_invalid_address() {
        let state = crate::test_support::app_state_with_storage(Arc::new(
            crate::test_support::EmptyStorage,
        ));
        let resp = parcels_permissions_batch(
            State(state),
            Path("not-an-address".to_string()),
            Json(BatchParcelPermissionsRequest {
                parcels: vec!["0,0".to_string()],
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // The stub state carries no squid pool: the batch must answer the same 503
    // `land_source_unavailable` as the single route, and only AFTER request
    // validation (a bad request is a 400 even on a pool-less node).
    #[tokio::test]
    async fn batch_route_without_squid_pool_is_503() {
        let state = crate::test_support::app_state_with_storage(Arc::new(
            crate::test_support::EmptyStorage,
        ));
        let addr = "0x1111111111111111111111111111111111111111";

        let resp = parcels_permissions_batch(
            State(state.clone()),
            Path(addr.to_string()),
            Json(BatchParcelPermissionsRequest {
                parcels: vec!["0,0".to_string()],
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            body_text(resp)
                .await
                .contains("LAND rights are unavailable"),
            "must be the shared land_source_unavailable body"
        );

        let resp = parcels_permissions_batch(
            State(state),
            Path(addr.to_string()),
            Json(BatchParcelPermissionsRequest {
                parcels: vec!["moon".to_string()],
            }),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "validation answers before the pool check"
        );
    }
}
