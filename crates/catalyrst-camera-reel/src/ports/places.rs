use std::fmt;
use std::time::Duration;

use moka::future::Cache;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct PlacesApiResponse {
    #[allow(dead_code)]
    ok: bool,
    total: usize,
    data: Vec<PlaceEntry>,
}

#[derive(Deserialize, Debug)]
struct PlaceEntry {
    id: String,
}

#[derive(Debug)]
pub enum PlacesClientError {
    RequestFailed(reqwest::Error),
    ApiError(u16),
    ParseError(reqwest::Error),
    TaskFailed(tokio::task::JoinError),
}

impl fmt::Display for PlacesClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlacesClientError::RequestFailed(e) => write!(f, "request failed: {e}"),
            PlacesClientError::ApiError(status) => write!(f, "places API returned status {status}"),
            PlacesClientError::ParseError(e) => write!(f, "failed to parse response: {e}"),
            PlacesClientError::TaskFailed(e) => write!(f, "page fetch task failed: {e}"),
        }
    }
}

const PAGE: usize = 100;

pub struct PlacesClient {
    client: reqwest::Client,
    base_url: String,
    cache: Cache<String, Vec<String>>,
}

impl PlacesClient {
    pub fn new(base_url: String, ttl_seconds: u64, max_size: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");

        let cache = Cache::builder()
            .max_capacity(max_size)
            .time_to_live(Duration::from_secs(ttl_seconds))
            .build();

        Self {
            client,
            base_url,
            cache,
        }
    }

    async fn fetch_page(
        client: &reqwest::Client,
        base_url: &str,
        world_name: &str,
        offset: usize,
    ) -> Result<PlacesApiResponse, PlacesClientError> {
        let url = format!(
            "{}/api/places?names={}&limit={}&offset={}",
            base_url, world_name, PAGE, offset
        );
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(PlacesClientError::RequestFailed)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(PlacesClientError::ApiError(status));
        }
        response.json().await.map_err(PlacesClientError::ParseError)
    }

    /// The first page carries `total`; any further pages are fetched concurrently.
    pub async fn get_world_place_ids(
        &self,
        world_name: &str,
    ) -> Result<Vec<String>, PlacesClientError> {
        if let Some(cached) = self.cache.get(world_name).await {
            return Ok(cached);
        }

        let first = Self::fetch_page(&self.client, &self.base_url, world_name, 0).await?;
        let mut all_ids: Vec<String> = first.data.into_iter().map(|e| e.id).collect();

        if !all_ids.is_empty() && all_ids.len() < first.total {
            let mut pages = tokio::task::JoinSet::new();
            for offset in (all_ids.len()..first.total).step_by(PAGE) {
                let client = self.client.clone();
                let base_url = self.base_url.clone();
                let world_name = world_name.to_string();
                pages.spawn(async move {
                    Self::fetch_page(&client, &base_url, &world_name, offset)
                        .await
                        .map(|page| (offset, page))
                });
            }
            let mut rest = Vec::new();
            while let Some(joined) = pages.join_next().await {
                let (offset, page) = joined.map_err(PlacesClientError::TaskFailed)??;
                rest.push((offset, page));
            }
            rest.sort_by_key(|(offset, _)| *offset);
            all_ids.extend(
                rest.into_iter()
                    .flat_map(|(_, page)| page.data)
                    .map(|e| e.id),
            );
        }

        self.cache
            .insert(world_name.to_string(), all_ids.clone())
            .await;

        Ok(all_ids)
    }
}
