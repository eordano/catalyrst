//! Places-API client — port of social-service-ea `adapters/places-api.ts` plus
//! the `validateOwnership` logic from `logic/community/places.ts`.
//!
//! `getDestinations` POSTs the place-id + world-name set to
//! `${PLACES_API_URL}/api/destinations` and reads back `data[].owner`. Ownership
//! holds iff every requested id resolves to a place owned by the caller.

use serde::Deserialize;

/// UUID v4 shape (place ids); world names end in `.eth`. Mirrors upstream's
/// `UUID_REGEX` / `endsWith('.eth')` split used to address the places API.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

#[derive(Deserialize)]
struct DestinationsResponse {
    #[serde(default)]
    data: Vec<Destination>,
}

#[derive(Deserialize)]
struct Destination {
    id: String,
    #[serde(default)]
    owner: Option<String>,
}

#[derive(Debug)]
pub enum PlacesError {
    /// The configured caller does not own every requested place. Mirrors
    /// upstream's `NotAuthorizedError("...doesn't own all the places")`.
    NotOwner(String),
    /// Transport/parse failure talking to the places API.
    Upstream(String),
    /// `PLACES_API_URL` is unset: ownership cannot be verified.
    Unconfigured,
}

pub struct PlacesApiClient {
    client: reqwest::Client,
    base_url: Option<String>,
}

impl PlacesApiClient {
    pub fn new(base_url: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");
        Self {
            base_url: base_url.map(|u| u.trim_end_matches('/').to_string()),
            client,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.base_url.is_some()
    }

    /// `getDestinations(placeIds, worldNames)` — returns `(id, owner)` rows.
    async fn get_destinations(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Option<String>)>, PlacesError> {
        let Some(base) = &self.base_url else {
            return Err(PlacesError::Unconfigured);
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/api/destinations", base);
        let resp = self
            .client
            .post(&url)
            .json(&ids)
            .send()
            .await
            .map_err(|e| PlacesError::Upstream(format!("request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(PlacesError::Upstream(format!(
                "places API returned status {}",
                resp.status().as_u16()
            )));
        }
        let body: DestinationsResponse = resp
            .json()
            .await
            .map_err(|e| PlacesError::Upstream(format!("failed to parse response: {e}")))?;
        Ok(body.data.into_iter().map(|d| (d.id, d.owner)).collect())
    }

    /// `validateOwnership(placeIds, userAddress)` — succeeds (returning the owned
    /// place ids) iff `user` owns every distinct requested place. Empty input is
    /// trivially valid, matching upstream.
    pub async fn validate_ownership(
        &self,
        place_ids: &[String],
        user: &str,
    ) -> Result<Vec<String>, PlacesError> {
        if place_ids.is_empty() {
            return Ok(Vec::new());
        }

        // dedup, preserving the UUID / world-name split upstream applies before
        // addressing the places API.
        let mut unique: Vec<String> = Vec::new();
        for id in place_ids {
            if !unique.contains(id) {
                unique.push(id.clone());
            }
        }
        let addressable: Vec<String> = unique
            .iter()
            .filter(|id| is_uuid(id) || id.ends_with(".eth"))
            .cloned()
            .collect();

        let places = self.get_destinations(&addressable).await?;
        let user_lc = user.to_lowercase();

        let mut owned: Vec<String> = Vec::new();
        for (id, owner) in places {
            if owner
                .as_deref()
                .map(|o| o.eq_ignore_ascii_case(&user_lc))
                .unwrap_or(false)
            {
                owned.push(id);
            }
        }

        if owned.len() != unique.len() {
            return Err(PlacesError::NotOwner(format!(
                "The user {} doesn't own all the places",
                user
            )));
        }
        Ok(owned)
    }
}
