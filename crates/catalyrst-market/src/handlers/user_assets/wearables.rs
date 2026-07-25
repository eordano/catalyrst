use axum::extract::{Path, Query, State};
use axum::Json;

use super::{create_paginated_response, AssetsHttpResponse};
use crate::http::response::ApiError;
use crate::ports::user_assets::{
    fix_urn, parse_user_assets_params, GroupedWearable, ProfileWearable, UrnToken,
};
use crate::AppState;

pub async fn get_user_wearables(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<ProfileWearable>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data_with_prov, total, total_items) = state
        .user_assets
        .get_wearables_by_owner(&owner, filters.first, filters.skip)
        .await?;

    let grants = state.usage_grants.get_active_grants_for(&owner).await;
    let unlock_by_urn = build_unlock_by_urn(&grants);
    let data = super::apply_leases(data_with_prov, &unlock_by_urn);

    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        Some(total_items),
    )))
}

pub async fn get_user_wearables_urn_token(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<UrnToken>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data, total) = state
        .user_assets
        .get_owned_wearables_urn_and_token_id(&owner, filters.first, filters.skip)
        .await?;
    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        None,
    )))
}

pub async fn get_user_grouped_wearables(
    State(state): State<AppState>,
    Path((address,)): Path<(String,)>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<AssetsHttpResponse<GroupedWearable>>, ApiError> {
    let filters = parse_user_assets_params(&pairs);
    let owner = address.to_lowercase();
    let (data_with_prov, total) = state
        .user_assets
        .get_grouped_wearables_by_owner(&owner, &filters)
        .await?;

    let grants = state.usage_grants.get_active_grants_for(&owner).await;
    let unlock_by_urn = build_unlock_by_urn(&grants);
    let data = super::apply_leases(data_with_prov, &unlock_by_urn);

    Ok(Json(create_paginated_response(
        data,
        total,
        filters.first,
        filters.skip,
        None,
    )))
}

pub(super) fn build_unlock_by_urn(
    grants: &[crate::ports::usage_grants::UsageGrantStatus],
) -> std::collections::HashMap<String, i64> {
    let mut map: std::collections::HashMap<String, i64> =
        std::collections::HashMap::with_capacity(grants.len());
    for g in grants {
        map.entry(fix_urn(&g.urn)).or_insert(g.unlock_at);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::build_unlock_by_urn;
    use crate::ports::usage_grants::UsageGrantStatus;

    fn grant(urn: &str, unlock_at: i64) -> UsageGrantStatus {
        UsageGrantStatus {
            urn: urn.to_string(),
            token_id: None,
            category: "wearable".to_string(),
            status: "leased".to_string(),
            unlock_at,
        }
    }

    #[test]
    fn empty_grants_yield_empty_map() {
        assert!(build_unlock_by_urn(&[]).is_empty());
    }

    #[test]
    fn normalizes_mainnet_urn_to_ethereum_for_matching() {
        let urn = "urn:decentraland:mainnet:collections-v2:0xabc:0";
        let m = build_unlock_by_urn(&[grant(urn, 1_700_000_000_000)]);
        assert_eq!(
            m.get("urn:decentraland:ethereum:collections-v2:0xabc:0"),
            Some(&1_700_000_000_000)
        );
        assert!(!m.contains_key(urn));
    }

    #[test]
    fn duplicate_urn_keeps_first_most_recent() {
        let urn = "urn:decentraland:ethereum:collections-v2:0xabc:0";
        let m = build_unlock_by_urn(&[grant(urn, 111), grant(urn, 222)]);
        assert_eq!(m.get(urn), Some(&111));
    }
}
