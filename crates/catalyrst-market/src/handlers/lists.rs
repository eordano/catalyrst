use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::pagination::get_pagination_params;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::lists::FavoriteList as ListRow;
use crate::AppState;

#[derive(Debug, Serialize)]
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
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub updated_at: Option<i64>,
    #[serde(rename = "isPrivate")]
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub permission: Option<String>,
    #[serde(rename = "itemsCount")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub items_count: i64,
    #[serde(rename = "previewOfItemIds")]
    pub preview_of_item_ids: Vec<String>,
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
            items_count: r.items_count,
            preview_of_item_ids: r.preview_of_item_ids,
        }
    }
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ListsEnvelope {
    pub ok: bool,
    pub data: ListsPage,
}

pub async fn get_lists(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ListsEnvelope>, ApiError> {
    let pg = get_pagination_params(&pairs);
    let p = Params::new(&pairs);
    let user_address = p.get_string("userAddress", None);

    let (results, total) = state
        .lists
        .get_lists(user_address.as_deref(), pg.limit, pg.offset)
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
            items_count: 3,
            preview_of_item_ids: vec![
                "0xf1483f042614105cb943d3dd67157256cd003028-15".to_string(),
                "0xf1483f042614105cb943d3dd67157256cd003028-16".to_string(),
            ],
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
            items_count: 0,
            preview_of_item_ids: vec![],
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
                "itemsCount": 0,
                "previewOfItemIds": [],
            },
        ]);
        assert_eq!(old["data"]["results"], expected_rows);
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
