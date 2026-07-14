//! Typed wire shapes for the world-storage REST responses.
//!
//! These are the payloads `sites` consumes (creator-hub/worlds-storage.ts):
//! the `/usage/*` size reports, the `{ data, pagination }` envelopes of the
//! values/env/players listings, and the value rows inside them. The raw-splice
//! listing path in `handlers::common` stays byte-identical for stored JSON;
//! `ValuesListResponse` mirrors its shape and a test below pins the two
//! together. Note the wire truth the generated TS exposes: `GET /env` and
//! `GET /players` both serve a plain string list under `data`, not key
//! objects.

use serde::Serialize;
use serde_json::Value;

/// Body of `GET /usage/world`, `GET /usage/env` and
/// `GET /usage/players/{player}`.
#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "world-storage/")
)]
pub struct UsageResponse {
    #[serde(rename = "usedBytes")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub used_bytes: i64,
    #[serde(rename = "maxTotalSizeBytes")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub max_total_size_bytes: i64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "world-storage/")
)]
pub struct PaginationInfo {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub limit: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub offset: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

/// Body of `GET /env` (env variable names) and `GET /players`
/// (player addresses): a paginated plain string list.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "world-storage/")
)]
pub struct KeyListResponse {
    pub data: Vec<String>,
    pub pagination: PaginationInfo,
}

/// One row of a values listing: the stored key plus its arbitrary JSON value.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "world-storage/")
)]
pub struct StorageValueRow {
    pub key: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub value: Value,
}

/// Body of `GET /values` and `GET /players/{player}`. Served by the
/// byte-preserving raw splice (`handlers::common::raw_paginated_response`);
/// a unit test in the emitting crate keeps this struct and the splice in
/// lockstep.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "world-storage/")
)]
pub struct ValuesListResponse {
    pub data: Vec<StorageValueRow>,
    pub pagination: PaginationInfo,
}

#[cfg(test)]
mod tests {
    use super::{PaginationInfo, StorageValueRow, UsageResponse, ValuesListResponse};
    use crate::world_storage::handlers::common::raw_paginated_response;
    use crate::world_storage::storage::StorageEntry;
    use serde_json::json;

    #[test]
    fn usage_response_serializes_like_the_upstream_json() {
        assert_eq!(
            serde_json::to_value(UsageResponse {
                used_bytes: 5,
                max_total_size_bytes: 10,
            })
            .unwrap(),
            json!({ "usedBytes": 5, "maxTotalSizeBytes": 10 })
        );
    }

    #[test]
    fn values_list_shape_matches_the_raw_splice() {
        let values = [json!({"n": 1, "s": "x"}), json!(true), json!([1, 2])];
        let entries: Vec<StorageEntry> = values
            .iter()
            .enumerate()
            .map(|(i, v)| StorageEntry {
                key: format!("k{i}"),
                value: serde_json::to_string(v).unwrap(),
            })
            .collect();
        let typed = ValuesListResponse {
            data: entries
                .iter()
                .map(|e| StorageValueRow {
                    key: e.key.clone(),
                    value: serde_json::from_str(&e.value).unwrap(),
                })
                .collect(),
            pagination: PaginationInfo {
                limit: 100,
                offset: 0,
                total: 3,
            },
        };
        assert_eq!(
            serde_json::to_string(&typed).unwrap(),
            raw_paginated_response(&entries, 100, 0, 3).0
        );
    }
}
