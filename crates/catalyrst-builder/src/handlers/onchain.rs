use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ItemsParams {
    #[serde(rename = "onlyOrphans")]
    pub only_orphans: Option<bool>,
}

fn valid_address(addr: &str) -> bool {
    let a = addr.strip_prefix("0x").unwrap_or(addr);
    a.len() == 40 && a.chars().all(|c| c.is_ascii_hexdigit())
}

const MARKETPLACE_DOWN: &str = "marketplace data unavailable";

pub async fn get_address_collections(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<ApiData<Vec<Value>>>, ApiError> {
    if !valid_address(&address) {
        return Err(ApiError::bad_request("Invalid address"));
    }
    let mp = state
        .marketplace
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable(MARKETPLACE_DOWN))?;
    let data = mp.collections_for_address(&address).await?;
    Ok(Json(ApiData::ok(data)))
}

pub async fn get_address_items(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(params): Query<ItemsParams>,
) -> Result<Json<ApiData<Vec<Value>>>, ApiError> {
    if !valid_address(&address) {
        return Err(ApiError::bad_request("Invalid address"));
    }
    let only_orphans = params.only_orphans.unwrap_or(false);
    let mp = state
        .marketplace
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable(MARKETPLACE_DOWN))?;
    let data = mp.items_for_address(&address, only_orphans).await?;
    Ok(Json(ApiData::ok(data)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::{Path, Query, State};
    use axum::response::IntoResponse;

    use super::*;
    use crate::ports::items::{ItemsComponent, NewsletterComponent};
    use crate::ports::marketplace::MarketplaceComponent;
    use crate::{AppState, AppStateInner};

    const ADDR: &str = "0x797066a17f83425c1b4c7a8cca52d19095520a52";

    fn dead_pool() -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(250))
            .connect_lazy("postgres://nobody:nothing@127.0.0.1:9/none")
            .expect("lazy pool from static url")
    }

    fn state(marketplace: Option<MarketplaceComponent>) -> AppState {
        Arc::new(AppStateInner {
            items: ItemsComponent::new(dead_pool()),
            newsletter: NewsletterComponent::new(dead_pool()),
            marketplace,
            content_bucket_url: "https://example.test".into(),
            admin_addresses: Vec::new(),
            newsletter_service_url: None,
            newsletter_publication_id: None,
            newsletter_api_key: None,
            admin_token: None,
            http: reqwest::Client::new(),
        })
    }

    fn status_of(err: ApiError) -> u16 {
        err.into_response().status().as_u16()
    }

    #[tokio::test]
    async fn collections_return_503_when_marketplace_component_is_absent() {
        let res = get_address_collections(State(state(None)), Path(ADDR.to_string())).await;
        let err = res.expect_err("must be an error, not an empty 200");
        assert_eq!(status_of(err), 503);
    }

    #[tokio::test]
    async fn items_return_503_when_marketplace_component_is_absent() {
        let res = get_address_items(
            State(state(None)),
            Path(ADDR.to_string()),
            Query(ItemsParams::default()),
        )
        .await;
        let err = res.expect_err("must be an error, not an empty 200");
        assert_eq!(status_of(err), 503);
    }

    #[tokio::test]
    async fn collections_surface_db_failure_as_5xx_not_empty_200() {
        let mp = Some(MarketplaceComponent::new(dead_pool()));
        let res = get_address_collections(State(state(mp)), Path(ADDR.to_string())).await;
        let err = res.expect_err("a dead marketplace DB must error, never yield an empty list");
        assert_eq!(status_of(err), 500);
    }

    #[tokio::test]
    async fn items_surface_db_failure_as_5xx_not_empty_200() {
        let mp = Some(MarketplaceComponent::new(dead_pool()));
        let res = get_address_items(
            State(state(mp)),
            Path(ADDR.to_string()),
            Query(ItemsParams::default()),
        )
        .await;
        let err = res.expect_err("a dead marketplace DB must error, never yield an empty list");
        assert_eq!(status_of(err), 500);
    }

    #[tokio::test]
    async fn invalid_address_is_still_a_400() {
        let res = get_address_collections(State(state(None)), Path("nope".into())).await;
        assert_eq!(status_of(res.expect_err("invalid address must 400")), 400);
    }
}
