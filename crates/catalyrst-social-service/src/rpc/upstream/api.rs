use super::identity::UpstreamIdentity;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub struct UpstreamRestResponse {
    pub status: u16,
    pub body: Value,
}

pub struct UpstreamApi {
    base: String,
    identity: Arc<UpstreamIdentity>,
    client: reqwest::Client,
}

impl UpstreamApi {
    pub fn new(base_url: &str, identity: Arc<UpstreamIdentity>) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder().timeout(CALL_TIMEOUT).build()?;
        Ok(Self {
            base: base_url.trim_end_matches('/').to_string(),
            identity,
            client,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn sign_path(path_and_query: &str) -> &str {
        path_and_query.split('?').next().unwrap_or(path_and_query)
    }

    pub async fn get_json(&self, path_and_query: &str) -> anyhow::Result<UpstreamRestResponse> {
        let headers =
            self.identity
                .signed_fetch_headers("get", Self::sign_path(path_and_query), "{}")?;
        let mut req = self.client.get(format!("{}{}", self.base, path_and_query));
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let text = resp.text().await?;
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok(UpstreamRestResponse { status, body })
    }

    pub async fn get_communities(
        &self,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<UpstreamRestResponse> {
        self.get_json(&format!("/v1/communities?limit={limit}&offset={offset}"))
            .await
    }

    pub async fn get_community(&self, id: &str) -> anyhow::Result<UpstreamRestResponse> {
        self.get_json(&format!("/v1/communities/{id}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    fn api(base: &str) -> UpstreamApi {
        let identity = Arc::new(UpstreamIdentity::from_root_hex(KEY).unwrap());
        UpstreamApi::new(base, identity).unwrap()
    }

    #[test]
    fn sign_path_drops_the_query_string() {
        assert_eq!(
            UpstreamApi::sign_path("/v1/communities?limit=5"),
            "/v1/communities"
        );
        assert_eq!(UpstreamApi::sign_path("/v1/mutes"), "/v1/mutes");
    }

    #[test]
    fn construction_normalizes_the_base_and_stays_offline() {
        assert_eq!(api("https://up.example/").base(), "https://up.example");
        assert_eq!(api("https://up.example").base(), "https://up.example");
    }

    #[tokio::test]
    async fn live_upstream_communities_read() {
        if catalyrst_testgate::env_value(super::super::session::LIVE_TEST_VAR).is_none() {
            return;
        }
        let cfg = super::super::UpstreamConfig::from_env()
            .expect("UPSTREAM_SOCIAL_LIVE_TEST is set but the UPSTREAM_SOCIAL_* config is not");
        let base = cfg.social_api_url.clone().expect("UPSTREAM_SOCIAL_API_URL");
        let identity = Arc::new(
            cfg.identity()
                .expect("identity from UPSTREAM_SOCIAL_KEY_FILE"),
        );
        let api = UpstreamApi::new(&base, identity).unwrap();
        let resp = api
            .get_communities(5, 0)
            .await
            .expect("GET /v1/communities");
        assert_eq!(resp.status, 200, "{:?}", resp.body);
    }
}
