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
    #[serde(rename = "totalItems", skip_serializing_if = "Option::is_none")]
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
    let page = skip / limit + 1;
    let pages = if limit > 0 {
        (total + limit - 1) / limit
    } else {
        0
    };
    AssetsHttpResponse {
        ok: true,
        data: PaginatedAssetsBody {
            elements,
            page,
            pages,
            limit,
            total,
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
    use super::apply_leases;
    use crate::ports::user_assets::{ProfileEmote, ProfileWearable};
    use std::collections::HashMap;

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
