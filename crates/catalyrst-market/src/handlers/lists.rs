use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::auth_chain::{self, AuthChainError, AuthChainErrorExt};
use crate::http::pagination::get_pagination_params;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::lists::{
    is_uuid, FavoriteList as ListRow, GetListsOptions, ListPick, ListSortBy, ListSortDirection,
};
use crate::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct FavoriteList {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    #[serde(rename = "userAddress")]
    pub user_address: String,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub updated_at: Option<i64>,
    #[serde(rename = "isPrivate")]
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub permission: Option<String>,
    /// True only for the globally shared default Wishlist (upstream's
    /// `is_default_list` projection; it also drives the sort-first contract).
    #[serde(rename = "isDefaultList")]
    pub is_default_list: bool,
    #[serde(rename = "itemsCount")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub items_count: i64,
    #[serde(rename = "previewOfItemIds")]
    pub preview_of_item_ids: Vec<String>,
    #[serde(rename = "isItemInList")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub is_item_in_list: Option<bool>,
}

impl From<ListRow> for FavoriteList {
    fn from(r: ListRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            user_address: r.user_address,
            created_at: r.created_at,
            updated_at: r.updated_at,
            is_private: r.is_private,
            permission: r.permission,
            is_default_list: r.is_default_list,
            items_count: r.items_count,
            preview_of_item_ids: r.preview_of_item_ids,
            is_item_in_list: r.is_item_in_list,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ListsPage {
    pub results: Vec<FavoriteList>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub page: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pages: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ListsEnvelope {
    pub ok: bool,
    pub data: ListsPage,
}

/// One favorited item inside a list, as `GET /v1/lists/{id}/picks` reports it
/// (upstream `fromDBGetPickByListIdToPickIdsWithCount`: item id + pick time).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ListPickItem {
    #[serde(rename = "itemId")]
    #[cfg_attr(feature = "ts", ts(rename = "itemId"))]
    pub item_id: String,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(rename = "createdAt", type = "number"))]
    pub created_at: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PicksPage {
    pub results: Vec<ListPickItem>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub page: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pages: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct PicksEnvelope {
    pub ok: bool,
    pub data: PicksPage,
}

fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::Http(catalyrst_types::HttpError::new(401, e.message())),
    }
}

fn parse_sort_by(raw: Option<&str>) -> Result<ListSortBy, ApiError> {
    match raw {
        None => Ok(ListSortBy::CreatedAt),
        Some("createdAt") => Ok(ListSortBy::CreatedAt),
        Some("name") => Ok(ListSortBy::Name),
        Some("updatedAt") => Ok(ListSortBy::UpdatedAt),
        Some(_) => Err(ApiError::bad_request(
            "The sort by parameter is not defined as createdAt, name, or updatedAt.",
        )),
    }
}

fn parse_sort_direction(raw: Option<&str>) -> Result<ListSortDirection, ApiError> {
    match raw {
        None => Ok(ListSortDirection::Desc),
        Some("asc") => Ok(ListSortDirection::Asc),
        Some("desc") => Ok(ListSortDirection::Desc),
        Some(_) => Err(ApiError::bad_request(
            "The sort direction parameter is not defined as asc or desc.",
        )),
    }
}

#[utoipa::path(
    get,
    path = "/v1/lists",
    tag = "market",
    responses(
        (status = 200, body = ListsEnvelope),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn get_lists(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ListsEnvelope>, ApiError> {
    // @dcl/crypto-middleware >=5.1.0: reject non-canonical signer/intent metadata
    // (mixed case or whitespace) with 400 before the signature is validated.
    auth_chain::require_canonical_metadata(&headers)?;
    let user_address = auth_chain::require_signer(&headers, "get", "/v1/lists")
        .await
        .map_err(auth_chain_error_to_api)?
        .as_str()
        .to_string();

    let pg = get_pagination_params(&pairs);
    let p = Params::new(&pairs);
    let sort_by = parse_sort_by(p.get_string("sortBy", None).as_deref())?;
    let sort_direction = parse_sort_direction(p.get_string("sortDirection", None).as_deref())?;
    let item_id = p.get_string("itemId", None);
    let q = p.get_string("q", None);

    let (results, total) = state
        .lists
        .get_lists(
            &user_address,
            &GetListsOptions {
                limit: pg.limit,
                offset: pg.offset,
                sort_by,
                sort_direction,
                item_id: item_id.as_deref(),
                q: q.as_deref(),
            },
        )
        .await?;

    let page = if pg.limit > 0 {
        pg.offset / pg.limit
    } else {
        0
    };
    let pages = if total > 0 && pg.limit > 0 {
        (total + pg.limit - 1) / pg.limit
    } else {
        0
    };

    Ok(Json(ListsEnvelope {
        ok: true,
        data: ListsPage {
            results: results.into_iter().map(FavoriteList::from).collect(),
            total,
            page,
            pages,
            limit: pg.limit,
        },
    }))
}

/// Upstream envelope math (`getPicksByListIdHandler`): a page with no results
/// zeroes `total` and `pages` even when the pre-pagination count is not zero
/// (e.g. an offset past the end).
fn picks_page(picks: Vec<ListPick>, count: i64, limit: i64, offset: i64) -> PicksPage {
    let page = if limit > 0 { offset / limit } else { 0 };
    let pages = if !picks.is_empty() && limit > 0 {
        (count + limit - 1) / limit
    } else {
        0
    };
    let total = if picks.is_empty() { 0 } else { count };
    PicksPage {
        results: picks
            .into_iter()
            .map(|p| ListPickItem {
                item_id: p.item_id,
                created_at: p.created_at,
            })
            .collect(),
        total,
        page,
        pages,
        limit,
    }
}

#[utoipa::path(
    get,
    path = "/v1/lists/{id}/picks",
    tag = "market",
    params(("id" = String, Path)),
    responses(
        (status = 200, body = PicksEnvelope),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
pub async fn get_list_picks(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<PicksEnvelope>, ApiError> {
    let user_address =
        auth_chain::optional_signer(&headers, "get", &format!("/v1/lists/{id}/picks")).await?;

    // Guarded here (as POST /v1/picks does for body list ids) instead of
    // letting the `$1::uuid` bind blow up in postgres as a 500.
    if !is_uuid(&id) {
        return Err(ApiError::bad_request("The list id must be a UUID."));
    }

    let pg = get_pagination_params(&pairs);
    let (picks, count) = state
        .lists
        .get_picks_by_list_id(&id, user_address.as_deref(), pg.limit, pg.offset)
        .await?;

    Ok(Json(PicksEnvelope {
        ok: true,
        data: picks_page(picks, count, pg.limit, pg.offset),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn full_row() -> ListRow {
        ListRow {
            id: "6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0001".to_string(),
            name: "Summer fits".to_string(),
            description: Some("wearables I like".to_string()),
            user_address: "0x57d1721f6223eb434e20f5b4a88e494008ea0542".to_string(),
            created_at: 1_782_326_697_937,
            updated_at: Some(1_782_326_700_000),
            is_private: false,
            permission: Some("edit".to_string()),
            is_default_list: false,
            items_count: 3,
            preview_of_item_ids: vec![
                "0xf1483f042614105cb943d3dd67157256cd003028-15".to_string(),
                "0xf1483f042614105cb943d3dd67157256cd003028-16".to_string(),
            ],
            is_item_in_list: None,
        }
    }

    fn minimal_row() -> ListRow {
        ListRow {
            id: "6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0002".to_string(),
            name: "empty".to_string(),
            description: None,
            user_address: "0xabc".to_string(),
            created_at: 1,
            updated_at: None,
            is_private: true,
            permission: None,
            is_default_list: false,
            items_count: 0,
            preview_of_item_ids: vec![],
            is_item_in_list: None,
        }
    }

    #[test]
    fn wire_identity_lists_envelope() {
        let port_rows = vec![full_row(), minimal_row()];
        let old = json!({
            "ok": true,
            "data": {
                "results": port_rows,
                "total": 2,
                "page": 0,
                "pages": 1,
                "limit": 100,
            }
        });

        let new = ListsEnvelope {
            ok: true,
            data: ListsPage {
                results: vec![full_row(), minimal_row()]
                    .into_iter()
                    .map(FavoriteList::from)
                    .collect(),
                total: 2,
                page: 0,
                pages: 1,
                limit: 100,
            },
        };
        assert_eq!(serde_json::to_value(&new).unwrap(), old);

        let expected_rows = json!([
            {
                "id": "6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0001",
                "name": "Summer fits",
                "description": "wearables I like",
                "userAddress": "0x57d1721f6223eb434e20f5b4a88e494008ea0542",
                "createdAt": 1_782_326_697_937_i64,
                "updatedAt": 1_782_326_700_000_i64,
                "isPrivate": false,
                "permission": "edit",
                "isDefaultList": false,
                "itemsCount": 3,
                "previewOfItemIds": [
                    "0xf1483f042614105cb943d3dd67157256cd003028-15",
                    "0xf1483f042614105cb943d3dd67157256cd003028-16",
                ],
            },
            {
                "id": "6a0e4b1e-0f6e-4c7a-9d2b-2f1c9a1a0002",
                "name": "empty",
                "userAddress": "0xabc",
                "createdAt": 1,
                "isPrivate": true,
                "isDefaultList": false,
                "itemsCount": 0,
                "previewOfItemIds": [],
            },
        ]);
        assert_eq!(old["data"]["results"], expected_rows);
    }

    #[test]
    fn default_wishlist_row_serializes_is_default_list_true() {
        let mut row = minimal_row();
        row.id = crate::ports::lists::DEFAULT_LIST_ID.to_string();
        row.user_address = crate::ports::lists::DEFAULT_LIST_USER_ADDRESS.to_string();
        row.is_default_list = true;
        let v = serde_json::to_value(FavoriteList::from(row)).unwrap();
        assert_eq!(v["isDefaultList"], json!(true));
        assert_eq!(
            v["userAddress"],
            json!("0x0000000000000000000000000000000000000000")
        );
    }

    #[test]
    fn picks_envelope_matches_upstream_wire_shape() {
        let picks = vec![
            ListPick {
                item_id: "0xf1483f042614105cb943d3dd67157256cd003028-15".to_string(),
                created_at: 1_782_326_697_937,
            },
            ListPick {
                item_id: "0xf1483f042614105cb943d3dd67157256cd003028-16".to_string(),
                created_at: 1_782_326_700_000,
            },
        ];
        let env = PicksEnvelope {
            ok: true,
            data: picks_page(picks, 5, 2, 2),
        };
        assert_eq!(
            serde_json::to_value(&env).unwrap(),
            json!({
                "ok": true,
                "data": {
                    "results": [
                        { "itemId": "0xf1483f042614105cb943d3dd67157256cd003028-15", "createdAt": 1_782_326_697_937_i64 },
                        { "itemId": "0xf1483f042614105cb943d3dd67157256cd003028-16", "createdAt": 1_782_326_700_000_i64 },
                    ],
                    "total": 5,
                    "page": 1,
                    "pages": 3,
                    "limit": 2,
                }
            })
        );
    }

    #[test]
    fn picks_page_zeroes_total_and_pages_when_the_page_is_empty() {
        // Upstream: `total: picks.length > 0 ? count : 0` and
        // `pages: picks.length > 0 ? Math.ceil(count / limit) : 0` -- an offset
        // past the end reports 0/0 even though the count was nonzero.
        let page = picks_page(Vec::new(), 5, 2, 10);
        assert_eq!(page.total, 0);
        assert_eq!(page.pages, 0);
        assert_eq!(page.page, 5);
        assert_eq!(page.limit, 2);
        assert!(page.results.is_empty());
    }

    #[test]
    fn wire_identity_lists_envelope_empty_matches_baseline() {
        let new = ListsEnvelope {
            ok: true,
            data: ListsPage {
                results: vec![],
                total: 0,
                page: 0,
                pages: 0,
                limit: 100,
            },
        };
        let baseline: serde_json::Value = serde_json::from_str(
            r#"{"data":{"limit":100,"page":0,"pages":0,"results":[],"total":0},"ok":true}"#,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&new).unwrap(), baseline);
    }
}
