use std::time::Duration;

use axum::http::HeaderMap;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use serde_json::Value;

pub use catalyrst_fed::comms::CommsGatekeeper;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn destination_url(base_url: &str, id: &str) -> String {
    format!(
        "{}/v1/destinations/{}",
        base_url,
        utf8_percent_encode(id, NON_ALPHANUMERIC)
    )
}

pub(crate) fn place_lookup_url(base_url: &str, x: i32, y: i32) -> String {
    format!(
        "{}/api/places?positions={}&limit=1",
        base_url,
        utf8_percent_encode(&format!("{x},{y}"), NON_ALPHANUMERIC)
    )
}

pub(crate) fn world_lookup_url(base_url: &str, name: &str) -> String {
    format!(
        "{}/api/worlds?names={}&limit=1",
        base_url,
        utf8_percent_encode(name, NON_ALPHANUMERIC)
    )
}

pub(crate) fn is_world_event(world: bool, server: Option<&str>) -> bool {
    world && server.is_some_and(|s| !s.is_empty())
}

pub(crate) fn world_destination_id(row: &Value, requested: &str) -> String {
    row.get("world_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(requested)
        .to_lowercase()
}

#[derive(Debug, Deserialize)]
struct DestinationResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    data: Value,
}

fn forwarded_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for (name, value) in headers.iter() {
        if !name.as_str().starts_with("x-identity-") {
            continue;
        }
        let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) else {
            continue;
        };
        out.append(name, value);
    }
    out
}

pub struct Places {
    base_url: String,
    http: reqwest::Client,
}

impl Places {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn get_destination(&self, id: &str, headers: &HeaderMap) -> Option<Value> {
        let url = destination_url(&self.base_url, id);
        let resp = self
            .http
            .get(&url)
            .headers(forwarded_headers(headers))
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "places destination request failed");
                return None;
            }
        };
        if !resp.status().is_success() {
            return None;
        }
        match resp.json::<DestinationResponse>().await {
            Ok(parsed) if parsed.ok => parsed.data.filter(|d| !d.is_null()),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(error = %e, "places destination decode failed");
                None
            }
        }
    }

    pub async fn resolve_location(
        &self,
        world: bool,
        server: Option<&str>,
        x: i32,
        y: i32,
    ) -> (Option<String>, bool) {
        match server.filter(|_| is_world_event(world, server)) {
            Some(name) => {
                let id = self
                    .first_row(&world_lookup_url(&self.base_url, name))
                    .await
                    .map(|row| world_destination_id(&row, name));
                (id, true)
            }
            None => {
                let id = self
                    .first_row(&place_lookup_url(&self.base_url, x, y))
                    .await
                    .and_then(|row| {
                        row.get("id")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                    });
                (id, false)
            }
        }
    }

    pub async fn resolve_place_id(
        &self,
        world: bool,
        server: Option<&str>,
        x: i32,
        y: i32,
    ) -> Option<String> {
        self.resolve_location(world, server, x, y).await.0
    }

    async fn first_row(&self, url: &str) -> Option<Value> {
        let resp = match self.http.get(url).timeout(REQUEST_TIMEOUT).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "places lookup request failed");
                return None;
            }
        };
        if !resp.status().is_success() {
            return None;
        }
        match resp.json::<ListResponse>().await {
            Ok(parsed) if parsed.ok => parsed
                .data
                .as_array()
                .and_then(|rows| rows.first().cloned()),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(error = %e, "places lookup decode failed");
                None
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use axum::extract::Query;
    use axum::routing::get;
    use serde_json::json;

    pub(crate) async fn stub_places() -> String {
        let app = axum::Router::new()
            .route(
                "/api/places",
                get(|Query(q): Query<Vec<(String, String)>>| async move {
                    let hit = q.iter().any(|(k, v)| k == "positions" && v == "1,2");
                    let data = if hit {
                        json!([{ "id": "place-uuid", "world": false }])
                    } else {
                        json!([])
                    };
                    axum::Json(json!({ "ok": true, "data": data, "total": 0 }))
                }),
            )
            .route(
                "/api/worlds",
                get(|Query(q): Query<Vec<(String, String)>>| async move {
                    let hit = q
                        .iter()
                        .any(|(k, v)| k == "names" && v.eq_ignore_ascii_case("my-world.dcl.eth"));
                    let data = if hit {
                        json!([{ "id": "world-row-uuid", "world_name": "My-World.dcl.eth" }])
                    } else {
                        json!([])
                    };
                    axum::Json(json!({ "ok": true, "data": data, "total": 0 }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::stub_places;
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_auth_chain_is_forwarded_to_places() {
        let mut headers = HeaderMap::new();
        headers.insert("x-identity-auth-chain-0", "{}".parse().unwrap());
        headers.insert("x-identity-timestamp", "1".parse().unwrap());
        headers.insert("x-identity-metadata", "{}".parse().unwrap());
        headers.insert("authorization", "Bearer admin".parse().unwrap());
        headers.insert("cookie", "secret=1".parse().unwrap());
        let out = forwarded_headers(&headers);
        assert_eq!(out.len(), 3);
        assert!(out.contains_key("x-identity-auth-chain-0"));
        assert!(!out.contains_key("authorization"));
        assert!(!out.contains_key("cookie"));
    }

    #[test]
    fn a_destination_id_is_always_one_path_segment() {
        for (id, encoded) in [
            ("a/b", "a%2Fb"),
            ("x?y=1", "x%3Fy%3D1"),
            ("../../api/ping", "%2E%2E%2F%2E%2E%2Fapi%2Fping"),
            ("my-world.dcl.eth", "my%2Dworld%2Edcl%2Eeth"),
        ] {
            assert_eq!(
                destination_url("http://places", id),
                format!("http://places/v1/destinations/{encoded}"),
                "{id}"
            );
        }
    }

    #[test]
    fn lookup_urls_carry_one_encoded_filter_value() {
        assert_eq!(
            place_lookup_url("http://places", -12, 7),
            "http://places/api/places?positions=%2D12%2C7&limit=1"
        );
        assert_eq!(
            world_lookup_url("http://places", "my-world.dcl.eth&limit=100"),
            "http://places/api/worlds?names=my%2Dworld%2Edcl%2Eeth%26limit%3D100&limit=1"
        );
    }

    #[test]
    fn a_world_resolves_to_its_lowercased_name() {
        assert_eq!(
            world_destination_id(
                &json!({ "id": "row-uuid", "world_name": "My-World.dcl.eth" }),
                "x"
            ),
            "my-world.dcl.eth"
        );
        assert_eq!(
            world_destination_id(&json!({ "id": "row-uuid" }), "Requested.dcl.eth"),
            "requested.dcl.eth"
        );
        assert_eq!(
            world_destination_id(&json!({ "world_name": "" }), "Requested.dcl.eth"),
            "requested.dcl.eth"
        );
    }

    #[tokio::test]
    async fn resolve_location_derives_the_stored_world_flag() {
        let places = Places::new(stub_places().await);
        let parcel = (Some("place-uuid".to_string()), false);
        assert_eq!(places.resolve_location(true, None, 1, 2).await, parcel);
        assert_eq!(places.resolve_location(true, Some(""), 1, 2).await, parcel);
        assert_eq!(
            places
                .resolve_location(false, Some("my-world.dcl.eth"), 1, 2)
                .await,
            parcel
        );
        assert_eq!(
            places
                .resolve_location(true, Some("My-World.dcl.eth"), 1, 2)
                .await,
            (Some("my-world.dcl.eth".to_string()), true)
        );
        assert_eq!(
            places
                .resolve_location(true, Some("unknown.dcl.eth"), 1, 2)
                .await,
            (None, true),
            "an unknown world is still a world event, as upstream stores it"
        );
    }

    #[tokio::test]
    async fn resolve_place_id_follows_upstream_location_rules() {
        let places = Places::new(stub_places().await);
        assert_eq!(
            places.resolve_place_id(false, None, 1, 2).await.as_deref(),
            Some("place-uuid")
        );
        assert_eq!(
            places
                .resolve_place_id(false, Some("my-world.dcl.eth"), 1, 2)
                .await
                .as_deref(),
            Some("place-uuid"),
            "a server alone does not make a genesis event a world"
        );
        assert_eq!(
            places
                .resolve_place_id(true, Some(""), 1, 2)
                .await
                .as_deref(),
            Some("place-uuid"),
            "a world flag without a server falls back to the parcel"
        );
        assert_eq!(
            places
                .resolve_place_id(true, Some("My-World.dcl.eth"), 0, 0)
                .await
                .as_deref(),
            Some("my-world.dcl.eth")
        );
        assert_eq!(places.resolve_place_id(false, None, 0, 0).await, None);
        assert_eq!(
            places
                .resolve_place_id(true, Some("unknown.dcl.eth"), 0, 0)
                .await,
            None
        );
    }

    #[tokio::test]
    async fn an_unreachable_places_service_resolves_to_null() {
        let places = Places::new("http://127.0.0.1:9".into());
        assert_eq!(places.resolve_place_id(false, None, 1, 2).await, None);
        assert_eq!(
            places
                .resolve_place_id(true, Some("my-world.dcl.eth"), 0, 0)
                .await,
            None
        );
    }
}
