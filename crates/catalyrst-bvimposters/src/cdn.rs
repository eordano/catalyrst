use std::time::Duration;

use anyhow::{Context, Result};

use crate::key::ImposterKey;

pub fn upstream_url(base: &str, realm_segment: &str, key: &ImposterKey) -> String {
    format!(
        "{}/imposters/realms/{}/{}/{}",
        base.trim_end_matches('/'),
        realm_segment,
        key.tile.level,
        key.zip_name()
    )
}

pub struct CdnClient {
    http: reqwest::Client,
    base: String,
    realm_segment: String,
}

impl CdnClient {
    pub fn new(base: String, realm_segment: String, timeout_secs: u64) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("building cdn http client")?;
        Ok(Self {
            http,
            base,
            realm_segment,
        })
    }

    pub fn url_for(&self, key: &ImposterKey) -> String {
        upstream_url(&self.base, &self.realm_segment, key)
    }

    pub async fn fetch(&self, key: &ImposterKey) -> Result<Option<Vec<u8>>> {
        let url = self.url_for(key);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("cdn fetch {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            tracing::info!(%url, %status, "cdn miss");
            return Ok(None);
        }
        let bytes = resp
            .bytes()
            .await
            .with_context(|| format!("cdn body {url}"))?;
        Ok(Some(bytes.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_is_exact() {
        let key = ImposterKey::new(0, 0, 100, 3504527830).unwrap();
        let url = upstream_url(
            "https://bevy-imposters.dclregenesislabs.xyz",
            "https%253A%252F%252Frealm-provider-ea.decentraland.org%252Fmain%252Fabout",
            &key,
        );
        assert_eq!(
            url,
            "https://bevy-imposters.dclregenesislabs.xyz/imposters/realms/https%253A%252F%252Frealm-provider-ea.decentraland.org%252Fmain%252Fabout/0/0,100.3504527830.zip"
        );
    }

    #[test]
    fn upstream_url_negative_coords() {
        let key = ImposterKey::new(2, -64, -128, 7).unwrap();
        let url = upstream_url("https://cdn.example", "content", &key);
        assert_eq!(
            url,
            "https://cdn.example/imposters/realms/content/2/-64,-128.7.zip"
        );
    }
}
