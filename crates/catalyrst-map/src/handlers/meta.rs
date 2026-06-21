use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::cache;
use crate::AppState;

const IMAGE_BASE_URL: &str = "https://api.decentraland.org/v1";
const EXTERNAL_BASE_URL: &str = "https://market.decentraland.org";

fn finalize(mut resp: Response, last: i64) -> Response {
    cache::apply(&mut resp, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR);
    resp
}

fn not_ready() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "ok": false, "error": "Not Found" })),
    )
        .into_response()
}

fn internal_error(e: &sqlx::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "ok": false, "error": e.to_string() })),
    )
        .into_response()
}

pub async fn get_parcel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((x, y)): Path<(String, String)>,
) -> Response {
    let last = state.map.last_updated_at();
    if let Some(r) = cache::not_modified(&headers, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR)
    {
        return r;
    }
    finalize(get_parcel_inner(&state, x, y).await, last)
}

async fn get_parcel_inner(state: &AppState, x: String, y: String) -> Response {
    if !state.map.is_ready() {
        return not_ready();
    }
    let (Ok(xi), Ok(yi)) = (x.parse::<i32>(), y.parse::<i32>()) else {
        return (StatusCode::FORBIDDEN, "Invalid x or y").into_response();
    };

    let schema = &state.map_schema;
    let sql = format!(
        r#"
        SELECT p.token_id::text AS token_id, d.name AS name, d.description AS description
        FROM {schema}.parcel p
        LEFT JOIN {schema}.data d ON d.id = p.data_id
        WHERE p.x = $1 AND p.y = $2
        LIMIT 1
        "#
    );
    let row: Option<(String, Option<String>, Option<String>)> = match sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(xi as i64)
        .bind(yi as i64)
        .fetch_optional(&state.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => return internal_error(&e),
    };

    let Some((token_id, name, description)) = row else {
        return not_found();
    };

    let mut attributes: Vec<Value> = vec![
        json!({ "trait_type": "X", "value": xi, "display_type": "number" }),
        json!({ "trait_type": "Y", "value": yi, "display_type": "number" }),
    ];
    crate::proximity::append_attributes(&mut attributes, &[(xi, yi)]);

    let nft = json!({
        "id": token_id,
        "name": name.unwrap_or_else(|| format!("Parcel {},{}", xi, yi)),
        "description": description.unwrap_or_default(),
        "image": format!("{IMAGE_BASE_URL}/parcels/{xi}/{yi}/map.png?size=24&width=1024&height=1024"),
        "external_url": format!("{EXTERNAL_BASE_URL}/contracts/{}/tokens/{}", state.map.land_contract(), token_id),
        "background_color": "000000",
        "attributes": attributes,
    });
    (StatusCode::OK, Json(nft)).into_response()
}

pub async fn get_estate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let last = state.map.last_updated_at();
    if let Some(r) = cache::not_modified(&headers, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR)
    {
        return r;
    }
    finalize(get_estate_inner(&state, id).await, last)
}

async fn get_estate_inner(state: &AppState, id: String) -> Response {
    if !state.map.is_ready() {
        return not_ready();
    }
    if id.parse::<i64>().is_err() {
        return (StatusCode::FORBIDDEN, "Invalid id").into_response();
    }

    match build_estate_nft(state, &id).await {
        Ok(Some(nft)) => (StatusCode::OK, Json(nft)).into_response(),
        Ok(None) => match build_dissolved_estate(state, &id).await {
            Ok(Some(nft)) => (StatusCode::OK, Json(nft)).into_response(),
            Ok(None) => not_found(),
            Err(e) => internal_error(&e),
        },
        Err(e) => internal_error(&e),
    }
}

async fn build_estate_nft(state: &AppState, id: &str) -> Result<Option<Value>, sqlx::Error> {
    let schema = &state.map_schema;
    let full_id = format!("estate-{}-{}", state.map.estate_contract(), id);
    let sql = format!(
        r#"
        SELECT e.size AS size, n.name AS name, d.description AS description
        FROM {schema}.estate e
        LEFT JOIN {schema}.nft n ON n.id = $1 AND n.category = 'estate'
        LEFT JOIN {schema}.data d ON d.id = e.data_id
        WHERE e.id = $1
        LIMIT 1
        "#
    );
    let row: Option<(Option<i32>, Option<String>, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(&full_id)
        .fetch_optional(&state.pool)
        .await?;
    let Some((size, name, description)) = row else {
        return Ok(None);
    };

    let coords_sql = format!("SELECT x::int4, y::int4 FROM {schema}.parcel WHERE estate_id = $1");
    let coords: Vec<(i32, i32)> = sqlx::query_as(sqlx::AssertSqlSafe(coords_sql))
        .bind(&full_id)
        .fetch_all(&state.pool)
        .await?;

    let mut attributes: Vec<Value> =
        vec![json!({ "trait_type": "Size", "value": size.unwrap_or(0), "display_type": "number" })];
    crate::proximity::append_attributes(&mut attributes, &coords);

    Ok(Some(json!({
        "id": id,
        "name": name.unwrap_or_default(),
        "description": description.unwrap_or_default(),
        "image": format!("{IMAGE_BASE_URL}/estates/{id}/map.png?size=24&width=1024&height=1024"),
        "external_url": format!("{EXTERNAL_BASE_URL}/contracts/{}/tokens/{}", state.map.estate_contract(), id),
        "background_color": "000000",
        "attributes": attributes,
    })))
}

async fn build_dissolved_estate(state: &AppState, id: &str) -> Result<Option<Value>, sqlx::Error> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let schema = &state.map_schema;
    let full_id = format!("estate-{}-{}", state.map.estate_contract(), id);
    let sql = format!(
        r#"
        SELECT n.name AS name, d.description AS description
        FROM {schema}.estate e
        LEFT JOIN {schema}.nft n ON n.id = $1 AND n.category = 'estate'
        LEFT JOIN {schema}.data d ON d.id = e.data_id
        WHERE e.id = $1 AND e.size = 0
        LIMIT 1
        "#
    );
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(&full_id)
        .fetch_optional(&state.pool)
        .await?;
    let Some((name, description)) = row else {
        return Ok(None);
    };

    Ok(Some(json!({
        "id": id,
        "name": name.unwrap_or_default(),
        "description": description.unwrap_or_default(),
        "image": format!("{IMAGE_BASE_URL}/estates/{id}/map.png?size=24&width=1024&height=1024"),
        "external_url": format!("{EXTERNAL_BASE_URL}/contracts/{}/tokens/{}", state.map.estate_contract(), id),
        "background_color": "000000",
        "attributes": [
            { "trait_type": "Size", "value": 0, "display_type": "number" },
        ],
    })))
}

pub async fn get_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((address, id)): Path<(String, String)>,
) -> Response {
    let last = state.map.last_updated_at();
    if let Some(r) = cache::not_modified(&headers, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR)
    {
        return r;
    }
    let (mut resp, land_immutable) = get_token_inner(&state, address, id).await;
    if land_immutable && resp.status() == StatusCode::OK {
        cache::apply_with_cache_control(&mut resp, last, cache::LAND_IMMUTABLE_CACHE_CONTROL);
        resp
    } else {
        finalize(resp, last)
    }
}

async fn get_token_inner(state: &AppState, address: String, id: String) -> (Response, bool) {
    if !state.map.is_ready() {
        return (not_ready(), false);
    }
    let addr = address.to_lowercase();
    if addr == state.map.land_contract().to_lowercase() {
        let schema = &state.map_schema;
        let sql = format!(
            "SELECT x::int4, y::int4 FROM {schema}.parcel WHERE token_id = $1::numeric LIMIT 1"
        );
        let row: Option<(i32, i32)> = match sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(&id)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(r) => r,
            Err(e) => return (internal_error(&e), false),
        };
        if let Some((x, y)) = row {
            return (
                get_parcel_inner(state, x.to_string(), y.to_string()).await,
                true,
            );
        }
        return (not_found(), false);
    }
    if addr == state.map.estate_contract().to_lowercase() {
        if id.parse::<i64>().is_ok() {
            match build_estate_nft(state, &id).await {
                Ok(Some(nft)) => return ((StatusCode::OK, Json(nft)).into_response(), false),
                Ok(None) => {}
                Err(e) => return (internal_error(&e), false),
            }
            match build_dissolved_estate(state, &id).await {
                Ok(Some(nft)) => return ((StatusCode::OK, Json(nft)).into_response(), false),
                Ok(None) => {}
                Err(e) => return (internal_error(&e), false),
            }
        }
        return (not_found(), false);
    }
    (not_found(), false)
}

pub async fn get_districts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let last = state.map.last_updated_at();
    if let Some(r) = cache::not_modified(&headers, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR)
    {
        return r;
    }
    let resp = Json(json!({ "ok": true, "data": crate::districts::districts() })).into_response();
    finalize(resp, last)
}

pub async fn get_district(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let last = state.map.last_updated_at();
    if let Some(r) = cache::not_modified(&headers, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR)
    {
        return r;
    }
    let resp = match crate::districts::district(&id) {
        Some(d) => Json(json!({ "ok": true, "data": d })).into_response(),
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    finalize(resp, last)
}

pub async fn get_contributions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
) -> Response {
    let last = state.map.last_updated_at();
    if let Some(r) = cache::not_modified(&headers, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR)
    {
        return r;
    }
    let resp = Json(json!({ "ok": true, "data": crate::districts::contributions_for(&address) }))
        .into_response();
    finalize(resp, last)
}
