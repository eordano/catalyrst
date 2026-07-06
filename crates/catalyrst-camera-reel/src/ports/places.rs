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
}

impl fmt::Display for PlacesClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlacesClientError::RequestFailed(e) => write!(f, "request failed: {e}"),
            PlacesClientError::ApiError(status) => write!(f, "places API returned status {status}"),
            PlacesClientError::ParseError(e) => write!(f, "failed to parse response: {e}"),
        }
    }
}

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

    pub async fn get_world_place_ids(
        &self,
        world_name: &str,
    ) -> Result<Vec<String>, PlacesClientError> {
        if let Some(cached) = self.cache.get(world_name).await {
            return Ok(cached);
        }

        let mut all_ids = Vec::new();
        let mut offset: usize = 0;
        let limit: usize = 100;

        loop {
            let url = format!(
                "{}/api/places?names={}&limit={}&offset={}",
                self.base_url, world_name, limit, offset
            );

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(PlacesClientError::RequestFailed)?;

            let status = response.status().as_u16();
            if !response.status().is_success() {
                return Err(PlacesClientError::ApiError(status));
            }

            let body: PlacesApiResponse = response
                .json()
                .await
                .map_err(PlacesClientError::ParseError)?;

            for entry in &body.data {
                all_ids.push(entry.id.clone());
            }

            offset += body.data.len();
            if offset >= body.total || body.data.is_empty() {
                break;
            }
        }

        self.cache
            .insert(world_name.to_string(), all_ids.clone())
            .await;

        Ok(all_ids)
    }
}
