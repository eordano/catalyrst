use std::time::Duration;

use moka::future::Cache;
use serde::Deserialize;

use crate::http::errors::ApiError;

pub const GENESIS_CITY_REALM: &str = "main";

pub fn is_shared_realm(world_name: &str) -> bool {
    !world_name.to_ascii_lowercase().ends_with(".dcl.eth")
}

#[derive(Clone)]
pub struct ExternalClient {
    http: reqwest::Client,
    places_url: String,
    worlds_content_server_url: String,
    lambdas_url: String,
    place_id_cache: Cache<String, String>,
    world_permission_cache: Cache<String, bool>,
}

#[derive(Debug, Deserialize)]
struct PlacesApiResponse {
    #[serde(default)]
    data: Vec<PlaceEntry>,
}

#[derive(Debug, Deserialize)]
struct PlaceEntry {
    id: String,
}

#[derive(Debug, Deserialize)]
struct LandsParcelPermissions {
    #[serde(default)]
    owner: bool,
    #[serde(default)]
    operator: bool,
    #[serde(rename = "updateOperator", default)]
    update_operator: bool,
    #[serde(rename = "updateManager", default)]
    update_manager: bool,
    #[serde(rename = "approvedForAll", default)]
    approved_for_all: bool,
}

impl LandsParcelPermissions {
    fn has_any(&self) -> bool {
        self.owner
            || self.operator
            || self.update_operator
            || self.update_manager
            || self.approved_for_all
    }
}

#[derive(Debug, Deserialize)]
struct WorldPermissions {
    #[serde(default)]
    owner: Option<String>,
    permissions: WorldPermissionsInner,
}

#[derive(Debug, Deserialize)]
struct WorldPermissionsInner {
    deployment: DeploymentPermission,
}

#[derive(Debug, Deserialize)]
struct DeploymentPermission {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    wallets: Vec<String>,
}

fn is_world(realm: &str) -> bool {
    !is_shared_realm(realm)
}

impl ExternalClient {
    pub fn new(
        places_url: String,
        worlds_content_server_url: String,
        lambdas_url: String,
        places_cache_ttl_seconds: u64,
        world_permission_cache_ttl_seconds: u64,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        let place_id_cache = Cache::builder()
            .max_capacity(4096)
            .time_to_live(Duration::from_secs(places_cache_ttl_seconds))
            .build();
        let world_permission_cache = Cache::builder()
            .max_capacity(16384)
            .time_to_live(Duration::from_secs(world_permission_cache_ttl_seconds))
            .build();
        Self {
            http,
            places_url: places_url.trim_end_matches('/').to_string(),
            worlds_content_server_url: worlds_content_server_url.trim_end_matches('/').to_string(),
            lambdas_url: lambdas_url.trim_end_matches('/').to_string(),
            place_id_cache,
            world_permission_cache,
        }
    }

    async fn get_with_retry(&self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        match self.http.get(url).send().await {
            Ok(resp) if !resp.status().is_server_error() => Ok(resp),
            first => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                match self.http.get(url).send().await {
                    Ok(resp) => Ok(resp),
                    Err(retry_err) => Err(first.err().unwrap_or(retry_err)),
                }
            }
        }
    }

    pub async fn resolve_place_id(
        &self,
        world_name: &str,
        parcel: &str,
    ) -> Result<String, ApiError> {
        let cache_key = format!("{}:{}", world_name, parcel);
        if let Some(hit) = self.place_id_cache.get(&cache_key).await {
            return Ok(hit);
        }

        let encoded_parcel = urlencoding(parcel);
        let url = if is_world(world_name) {
            format!(
                "{}/api/places?names={}&positions={}",
                self.places_url,
                urlencoding(world_name),
                encoded_parcel
            )
        } else {
            format!(
                "{}/api/places?positions={}",
                self.places_url, encoded_parcel
            )
        };

        let resp = self
            .get_with_retry(&url)
            .await
            .map_err(|e| ApiError::internal(format!("Places API request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::internal(format!(
                "Places API returned HTTP {}",
                resp.status().as_u16()
            )));
        }
        let body: PlacesApiResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Places API bad body: {e}")))?;

        let place_id = body.data.into_iter().next().map(|p| p.id).ok_or_else(|| {
            ApiError::bad_request(format!(
                "Scene not found in Places API for world \"{}\" at parcel \"{}\"",
                world_name, parcel
            ))
        })?;

        self.place_id_cache
            .insert(cache_key, place_id.clone())
            .await;
        Ok(place_id)
    }

    pub async fn has_world_permission(
        &self,
        world_name: &str,
        address: &str,
        parcel: &str,
    ) -> Result<bool, ApiError> {
        let cache_key = format!("{}:{}:{}", world_name, address, parcel);
        if let Some(hit) = self.world_permission_cache.get(&cache_key).await {
            return Ok(hit);
        }
        let has = if is_world(world_name) {
            self.check_world_permission(world_name, address).await?
        } else {
            self.check_genesis_city_permission(address, parcel).await?
        };
        self.world_permission_cache.insert(cache_key, has).await;
        Ok(has)
    }

    async fn check_world_permission(
        &self,
        world_name: &str,
        address: &str,
    ) -> Result<bool, ApiError> {
        let url = format!(
            "{}/world/{}/permissions",
            self.worlds_content_server_url,
            urlencoding(world_name)
        );
        let resp = self.get_with_retry(&url).await.map_err(|e| {
            ApiError::internal(format!(
                "Failed to fetch world permissions for {world_name}: {e}"
            ))
        })?;
        if !resp.status().is_success() {
            return Err(ApiError::internal(format!(
                "Failed to fetch world permissions for {world_name}"
            )));
        }
        let perms: WorldPermissions = resp.json().await.map_err(|e| {
            ApiError::internal(format!("Bad world permissions body for {world_name}: {e}"))
        })?;

        if perms
            .owner
            .as_deref()
            .map(|o| o.to_ascii_lowercase() == address)
            .unwrap_or(false)
        {
            return Ok(true);
        }

        let is_deployer = perms.permissions.deployment.kind == "allow-list"
            && perms
                .permissions
                .deployment
                .wallets
                .iter()
                .any(|w| w.to_ascii_lowercase() == address);
        Ok(is_deployer)
    }

    async fn check_genesis_city_permission(
        &self,
        address: &str,
        parcel: &str,
    ) -> Result<bool, ApiError> {
        let mut parts = parcel.split(',');
        let x = parts.next().unwrap_or("0");
        let y = parts.next().unwrap_or("0");
        let url = format!(
            "{}/users/{}/parcels/{}/{}/permissions",
            self.lambdas_url,
            urlencoding(address),
            urlencoding(x),
            urlencoding(y)
        );
        let resp = match self.get_with_retry(&url).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, parcel, "Failed to check land permissions via LAMBDAS");
                return Err(ApiError::internal(format!("LAMBDAS request failed: {e}")));
            }
        };
        if !resp.status().is_success() {
            tracing::warn!(
                status = resp.status().as_u16(),
                parcel,
                "LAMBDAS permission check returned non-ok status"
            );
            return Ok(false);
        }
        let perms: LandsParcelPermissions = resp
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("LAMBDAS bad body: {e}")))?;
        Ok(perms.has_any())
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{is_shared_realm, is_world, GENESIS_CITY_REALM};

    #[test]
    fn shared_realm_is_anything_but_a_dcl_world_case_insensitively() {
        assert!(is_shared_realm(GENESIS_CITY_REALM));
        assert!(is_shared_realm("artemis"));
        assert!(!is_shared_realm("foo.dcl.eth"));
        assert!(!is_shared_realm("Foo.DCL.eth"));
        assert!(is_world("foo.dcl.eth"));
        assert!(!is_world("main"));
    }
}
