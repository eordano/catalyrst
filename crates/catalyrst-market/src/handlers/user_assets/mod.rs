pub mod emotes;
pub mod names;
pub mod wearables;

use serde::Serialize;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct AssetsHttpResponse<T> {
    pub ok: bool,
    pub data: PaginatedAssetsBody<T>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct PaginatedAssetsBody<T> {
    pub elements: Vec<T>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub page: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pages: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
    #[serde(rename = "totalItems")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number", optional))]
    pub total_items: Option<i64>,
}

pub fn create_paginated_response<T>(
    elements: Vec<T>,
    total: i64,
    first: i64,
    skip: i64,
    total_items: Option<i64>,
) -> AssetsHttpResponse<T> {
    let limit = if first == 0 { 1 } else { first };
    let page = catalyrst_types::PaginatedResponse::new_1based(elements, total, limit, skip);
    AssetsHttpResponse {
        ok: true,
        data: PaginatedAssetsBody {
            elements: page.results,
            page: page.page,
            pages: page.pages,
            limit: page.limit,
            total: page.total,
            total_items,
        },
    }
}

use crate::ports::user_assets::{GroupedEmote, GroupedWearable, ProfileEmote, ProfileWearable};

pub(super) trait Leasable {
    fn urn(&self) -> &str;
    fn mark_leased(&mut self, unlock_at: i64);
}

macro_rules! impl_leasable {
    ($t:ty) => {
        impl Leasable for $t {
            fn urn(&self) -> &str {
                &self.urn
            }
            fn mark_leased(&mut self, unlock_at: i64) {
                self.status = Some("leased".into());
                self.unlock_at = Some(unlock_at);
            }
        }
    };
}

impl_leasable!(ProfileWearable);
impl_leasable!(ProfileEmote);
impl_leasable!(GroupedWearable);
impl_leasable!(GroupedEmote);

pub(super) fn apply_leases<T: Leasable>(
    rows: Vec<(T, bool)>,
    unlock_by_urn: &std::collections::HashMap<String, i64>,
) -> Vec<T> {
    rows.into_iter()
        .map(|(mut el, is_leased)| {
            if is_leased {
                if let Some(&unlock_at) = unlock_by_urn.get(el.urn()) {
                    el.mark_leased(unlock_at);
                }
            }
            el
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{apply_leases, create_paginated_response};
    use crate::ports::user_assets::{ProfileEmote, ProfileWearable};
    use std::collections::HashMap;

    #[test]
    fn paginated_body_keeps_its_wire_shape() {
        let page = create_paginated_response(vec!["a", "b"], 10, 5, 5, Some(12));
        assert_eq!(
            serde_json::to_value(page).unwrap(),
            serde_json::json!({
                "ok": true,
                "data": {
                    "elements": ["a", "b"],
                    "page": 2,
                    "pages": 2,
                    "limit": 5,
                    "total": 10,
                    "totalItems": 12
                }
            })
        );
    }

    #[test]
    fn a_zero_limit_pages_one_element_at_a_time_and_omits_total_items() {
        let page = create_paginated_response(Vec::<&str>::new(), 7, 0, 3, None);
        assert_eq!(
            serde_json::to_value(page).unwrap(),
            serde_json::json!({
                "ok": true,
                "data": {
                    "elements": [],
                    "page": 4,
                    "pages": 7,
                    "limit": 1,
                    "total": 7
                }
            })
        );
    }

    #[test]
    fn the_first_page_is_one_based() {
        let page = create_paginated_response(Vec::<&str>::new(), 0, 100, 0, None);
        let body = serde_json::to_value(page).unwrap();
        assert_eq!(body["data"]["page"], 1);
        assert_eq!(body["data"]["pages"], 0);
    }

    fn wearable(urn: &str) -> ProfileWearable {
        ProfileWearable {
            urn: urn.to_string(),
            id: format!("{urn}:0"),
            token_id: "0".to_string(),
            category: "eyewear".to_string(),
            transferred_at: Some("0".to_string()),
            name: "Item".to_string(),
            rarity: "common".to_string(),
            price: None,
            status: None,
            unlock_at: None,
        }
    }

    fn emote(urn: &str) -> ProfileEmote {
        ProfileEmote {
            urn: urn.to_string(),
            id: format!("{urn}:0"),
            token_id: "0".to_string(),
            category: "dance".to_string(),
            transferred_at: Some("0".to_string()),
            name: "Emote".to_string(),
            rarity: "common".to_string(),
            price: None,
            status: None,
            unlock_at: None,
        }
    }

    #[test]
    fn owned_row_not_mislabeled_when_grant_shares_urn() {
        let urn = "urn:decentraland:ethereum:collections-v2:0xabc:0";
        let rows = vec![(wearable(urn), false), (wearable(urn), true)];
        let mut unlock_by_urn = HashMap::new();
        unlock_by_urn.insert(urn.to_string(), 1_700_000_000_000i64);

        let out = apply_leases(rows, &unlock_by_urn);

        assert_eq!(out[0].status, None);
        assert_eq!(out[0].unlock_at, None);
        assert_eq!(out[1].status.as_deref(), Some("leased"));
        assert_eq!(out[1].unlock_at, Some(1_700_000_000_000));
    }

    #[test]
    fn no_grants_is_byte_identity() {
        let urn = "urn:decentraland:ethereum:collections-v2:0xabc:0";
        let rows = vec![(wearable(urn), false), (wearable("urn:x:1"), false)];
        let unlock_by_urn: HashMap<String, i64> = HashMap::new();

        let out = apply_leases(rows, &unlock_by_urn);

        for el in &out {
            assert_eq!(el.status, None);
            assert_eq!(el.unlock_at, None);
        }
    }

    #[test]
    fn leased_row_missing_from_map_is_untouched() {
        let urn = "urn:decentraland:ethereum:collections-v2:0xabc:0";
        let rows = vec![(wearable(urn), true)];
        let unlock_by_urn: HashMap<String, i64> = HashMap::new();

        let out = apply_leases(rows, &unlock_by_urn);

        assert_eq!(out[0].status, None);
        assert_eq!(out[0].unlock_at, None);
    }

    #[test]
    fn emote_owned_row_not_mislabeled_when_grant_shares_urn() {
        let urn = "urn:decentraland:ethereum:collections-v2:0xabc:0";
        let rows = vec![(emote(urn), false), (emote(urn), true)];
        let mut unlock_by_urn = HashMap::new();
        unlock_by_urn.insert(urn.to_string(), 1_700_000_000_000i64);

        let out = apply_leases(rows, &unlock_by_urn);

        assert_eq!(out[0].status, None);
        assert_eq!(out[0].unlock_at, None);
        assert_eq!(out[1].status.as_deref(), Some("leased"));
        assert_eq!(out[1].unlock_at, Some(1_700_000_000_000));
    }
}
