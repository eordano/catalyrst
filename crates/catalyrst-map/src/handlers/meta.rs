use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::cache;
use crate::nft::{NftAttribute, NftMetadata};
use crate::AppState;

fn image_base_url() -> String {
    std::env::var("MAP_IMAGE_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:5162/v1".to_string())
}

/// Marketplace web page for a token. No production fallback: this stack does
/// not host a marketplace UI, so an unset value yields no external link
/// rather than one pointing at production.
fn external_base_url() -> Option<String> {
    std::env::var("MAP_EXTERNAL_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

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

fn ok_nft(nft: NftMetadata) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::to_value(nft).unwrap_or(Value::Null)),
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
    let row: Option<(String, Option<String>, Option<String>)> =
        match sqlx::query_as(sqlx::AssertSqlSafe(sql))
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

    ok_nft(parcel_nft(state, xi, yi, token_id, name, description))
}

/// The LAND-token metadata query: one statement that resolves coords AND
/// name/description from `token_id`, folding what used to be two sequential
/// round trips (token_id -> x,y, then x,y -> token_id,name,description).
fn land_token_sql(schema: &str) -> String {
    format!(
        r#"
        SELECT p.token_id::text, p.x::int4, p.y::int4, d.name, d.description
        FROM {schema}.parcel p
        LEFT JOIN {schema}.data d ON d.id = p.data_id
        WHERE p.token_id = $1::numeric
        LIMIT 1
        "#
    )
}

/// Shared by the direct `/v2/parcels/{x}/{y}` route and the LAND-token route so both
/// produce byte-identical bodies.
fn parcel_nft(
    state: &AppState,
    xi: i32,
    yi: i32,
    token_id: String,
    name: Option<String>,
    description: Option<String>,
) -> NftMetadata {
    let mut attributes: Vec<NftAttribute> = vec![
        NftAttribute {
            trait_type: "X".into(),
            value: xi as i64,
            display_type: "number".into(),
        },
        NftAttribute {
            trait_type: "Y".into(),
            value: yi as i64,
            display_type: "number".into(),
        },
    ];
    crate::proximity::append_attributes(&mut attributes, &[(xi, yi)]);

    let external_url = external_base_url().map(|b| {
        format!(
            "{b}/contracts/{}/tokens/{}",
            state.map.land_contract(),
            token_id
        )
    });
    NftMetadata {
        id: token_id,
        name: name.unwrap_or_else(|| format!("Parcel {},{}", xi, yi)),
        description: description.unwrap_or_default(),
        image: format!(
            "{}/parcels/{xi}/{yi}/map.png?size=24&width=1024&height=1024",
            image_base_url()
        ),
        external_url,
        background_color: "000000".into(),
        attributes,
    }
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
        Ok(Some(nft)) => ok_nft(nft),
        Ok(None) => match build_dissolved_estate(state, &id).await {
            Ok(Some(nft)) => ok_nft(nft),
            Ok(None) => not_found(),
            Err(e) => internal_error(&e),
        },
        Err(e) => internal_error(&e),
    }
}

async fn build_estate_nft(state: &AppState, id: &str) -> Result<Option<NftMetadata>, sqlx::Error> {
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
    let row: Option<(Option<i32>, Option<String>, Option<String>)> =
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
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

    let mut attributes: Vec<NftAttribute> = vec![NftAttribute {
        trait_type: "Size".into(),
        value: size.unwrap_or(0) as i64,
        display_type: "number".into(),
    }];
    crate::proximity::append_attributes(&mut attributes, &coords);

    Ok(Some(NftMetadata {
        id: id.to_string(),
        name: name.unwrap_or_default(),
        description: description.unwrap_or_default(),
        image: format!(
            "{}/estates/{id}/map.png?size=24&width=1024&height=1024",
            image_base_url()
        ),
        external_url: external_base_url().map(|b| {
            format!(
                "{b}/contracts/{}/tokens/{}",
                state.map.estate_contract(),
                id
            )
        }),
        background_color: "000000".into(),
        attributes,
    }))
}

async fn build_dissolved_estate(
    state: &AppState,
    id: &str,
) -> Result<Option<NftMetadata>, sqlx::Error> {
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

    Ok(Some(dissolved_estate_nft(
        id,
        name.unwrap_or_default(),
        description.unwrap_or_default(),
        state.map.estate_contract(),
    )))
}

fn dissolved_estate_nft(
    id: &str,
    name: String,
    description: String,
    estate_contract: &str,
) -> NftMetadata {
    NftMetadata {
        id: id.to_string(),
        name,
        description,
        image: format!(
            "{}/estates/{id}/map.png?size=24&width=1024&height=1024",
            image_base_url()
        ),
        external_url: external_base_url()
            .map(|b| format!("{b}/contracts/{estate_contract}/tokens/{id}")),
        background_color: "000000".into(),
        attributes: vec![NftAttribute {
            trait_type: "Size".into(),
            value: 0,
            display_type: "number".into(),
        }],
    }
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

type LandTokenRow = (String, i32, i32, Option<String>, Option<String>);

async fn get_token_inner(state: &AppState, address: String, id: String) -> (Response, bool) {
    if !state.map.is_ready() {
        return (not_ready(), false);
    }
    let addr = address.to_lowercase();
    if addr == state.map.land_contract().to_lowercase() {
        let sql = land_token_sql(&state.map_schema);
        let row: Option<LandTokenRow> = match sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(&id)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(r) => r,
            Err(e) => return (internal_error(&e), false),
        };
        if let Some((token_id, x, y, name, description)) = row {
            return (
                ok_nft(parcel_nft(state, x, y, token_id, name, description)),
                true,
            );
        }
        return (not_found(), false);
    }
    if addr == state.map.estate_contract().to_lowercase() {
        if id.parse::<i64>().is_ok() {
            match build_estate_nft(state, &id).await {
                Ok(Some(nft)) => return (ok_nft(nft), false),
                Ok(None) => {}
                Err(e) => return (internal_error(&e), false),
            }
            match build_dissolved_estate(state, &id).await {
                Ok(Some(nft)) => return (ok_nft(nft), false),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn land_token_metadata_is_one_query() {
        let sql = land_token_sql("squid_marketplace");
        let selects = sql.to_ascii_uppercase().matches("SELECT").count();
        assert_eq!(
            selects, 1,
            "LAND token metadata must be a single SELECT round trip"
        );
        assert!(
            sql.contains("LEFT JOIN"),
            "must join data in the same query"
        );
        assert!(
            sql.contains("d.name"),
            "must fetch name in the folded query"
        );
        assert!(
            sql.contains("d.description"),
            "must fetch description in the folded query"
        );
        assert!(sql.contains("p.token_id"), "must key on token_id");
        assert!(
            sql.contains("squid_marketplace.parcel"),
            "schema must be interpolated"
        );
    }

    #[test]
    fn dissolved_estate_fallback_shape() {
        let v = serde_json::to_value(dissolved_estate_nft(
            "42",
            "Old Estate".into(),
            "gone".into(),
            "0xestate",
        ))
        .unwrap();
        assert_eq!(v["id"], "42");
        assert_eq!(v["name"], "Old Estate");
        assert_eq!(v["description"], "gone");
        assert_eq!(v["background_color"], "000000");
        assert!(
            v["external_url"].is_null(),
            "with MAP_EXTERNAL_BASE_URL unset there must be no marketplace link, \
             never a production one: {}",
            v["external_url"]
        );
        assert_eq!(
            v["image"],
            "http://127.0.0.1:5162/v1/estates/42/map.png?size=24&width=1024&height=1024"
        );
        let attrs = v["attributes"].as_array().unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(
            attrs[0],
            json!({ "trait_type": "Size", "value": 0, "display_type": "number" })
        );
    }

    #[test]
    fn proximity_attributes_appended_for_known_coord() {
        let mut attrs: Vec<NftAttribute> = vec![
            NftAttribute {
                trait_type: "X".into(),
                value: 102,
                display_type: "number".into(),
            },
            NftAttribute {
                trait_type: "Y".into(),
                value: -33,
                display_type: "number".into(),
            },
        ];

        crate::proximity::append_attributes(&mut attrs, &[(102, -33)]);
        assert!(attrs.len() > 2, "expected proximity traits appended");
        for a in &attrs[2..] {
            assert!(
                a.trait_type.starts_with("Distance to "),
                "unexpected trait_type: {}",
                a.trait_type
            );
            assert_eq!(a.display_type, "number");
        }
        assert!(attrs
            .iter()
            .any(|a| a.trait_type == "Distance to Road" && a.value == 4));

        assert_eq!(attrs[2].trait_type, "Distance to Road");
    }

    #[test]
    fn proximity_noop_for_unknown_coord() {
        let mut attrs: Vec<NftAttribute> = vec![];
        crate::proximity::append_attributes(&mut attrs, &[(99999, 99999)]);
        assert!(attrs.is_empty());
    }
}
