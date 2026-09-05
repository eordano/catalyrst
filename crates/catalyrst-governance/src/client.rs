use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::parse;

const PROPOSALS_PAGE_SIZE: u64 = 100;
const REQUEST_DELAY: Duration = Duration::from_millis(200);
const MAX_ATTEMPTS: u32 = 10;
const USER_AGENT: &str = "catalyrst-governance/1.0 (+https://github.com/decentraland/catalyrst)";

pub struct GovernanceClient {
    base_url: String,
    http: reqwest::Client,
}

impl GovernanceClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        })
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Option<Value>> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(60);

        for _attempt in 0..MAX_ATTEMPTS {
            let req = self
                .http
                .get(&url)
                .query(query)
                .header(reqwest::header::ACCEPT, "application/json");

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.as_u16() == 429 {
                        let wait = retry_after(&resp).unwrap_or(Duration::from_secs(5));
                        tracing::warn!(%url, ?wait, "429; sleeping");
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                    if status.is_server_error() {
                        tracing::warn!(%url, code = status.as_u16(), ?backoff, "5xx; backing off");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(max_backoff);
                        continue;
                    }
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(anyhow!(
                            "governance http {} on {}: {}",
                            status.as_u16(),
                            url,
                            body.chars().take(200).collect::<String>()
                        ));
                    }
                    let bytes = resp.bytes().await?;
                    if bytes.is_empty() {
                        return Ok(None);
                    }
                    let value: Value = serde_json::from_slice(&bytes)?;
                    return Ok(Some(value));
                }
                Err(e) => {
                    tracing::warn!(%url, error = %e, ?backoff, "network error; backing off");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(max_backoff);
                }
            }
        }
        Err(anyhow!("exhausted retries on {}", url))
    }

    async fn throttle(&self) {
        tokio::time::sleep(REQUEST_DELAY).await;
    }

    pub async fn fetch_all_proposals(&self) -> Result<Vec<Value>> {
        let mut all = Vec::new();
        let mut offset: u64 = 0;
        loop {
            let resp = match self
                .get_json(
                    "proposals",
                    &[
                        ("limit", PROPOSALS_PAGE_SIZE.to_string()),
                        ("offset", offset.to_string()),
                    ],
                )
                .await?
            {
                Some(r) => r,
                None => break,
            };
            let (data, total) = parse::parse_page(&resp);
            if data.is_empty() {
                break;
            }
            all.extend(data);
            tracing::info!(fetched = all.len(), total, offset, "proposals page");
            offset += PROPOSALS_PAGE_SIZE;
            if offset >= total {
                break;
            }
            self.throttle().await;
        }
        Ok(all)
    }

    pub async fn fetch_recent_proposals(&self, hours: u32) -> Result<Vec<Value>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
        let mut all = Vec::new();
        let mut offset: u64 = 0;
        loop {
            let resp = match self
                .get_json(
                    "proposals",
                    &[
                        ("limit", PROPOSALS_PAGE_SIZE.to_string()),
                        ("offset", offset.to_string()),
                    ],
                )
                .await?
            {
                Some(r) => r,
                None => break,
            };
            let (data, total) = parse::parse_page(&resp);
            if data.is_empty() {
                break;
            }
            let oldest = data
                .last()
                .map(|p| parse::field(p, "updated_at").clone())
                .and_then(|v| parse::parse_ts(&v));
            all.extend(data);
            if let Some(ts) = oldest {
                if ts < cutoff {
                    tracing::info!(offset, oldest = %ts.to_rfc3339(), "reached sync cutoff");
                    break;
                }
            }
            offset += PROPOSALS_PAGE_SIZE;
            if offset >= total {
                break;
            }
            self.throttle().await;
        }
        Ok(all)
    }

    pub async fn fetch_projects(&self) -> Result<Vec<Value>> {
        match self.get_json("projects", &[]).await? {
            Some(resp) => Ok(parse::parse_data_array(&resp)),
            None => Ok(Vec::new()),
        }
    }

    pub async fn fetch_project_updates(&self, project_id: &str) -> Result<Vec<Value>> {
        match self
            .get_json("updates", &[("project_id", project_id.to_string())])
            .await?
        {
            Some(resp) => Ok(parse::parse_project_updates(&resp)),
            None => Ok(Vec::new()),
        }
    }

    pub async fn fetch_budgets(&self) -> Result<Vec<Value>> {
        match self.get_json("budget/all", &[]).await? {
            Some(resp) => Ok(parse::parse_list_or_data(&resp)),
            None => Ok(Vec::new()),
        }
    }

    pub async fn fetch_vestings(&self) -> Result<Vec<Value>> {
        match self.get_json("all-vestings", &[]).await? {
            Some(resp) => Ok(parse::parse_list_or_data(&resp)),
            None => Ok(Vec::new()),
        }
    }

    pub async fn fetch_members(&self, endpoint: &str) -> Result<Vec<String>> {
        match self.get_json(endpoint, &[]).await? {
            Some(resp) => Ok(parse::parse_members(&resp)),
            None => Ok(Vec::new()),
        }
    }

    pub async fn throttle_pub(&self) {
        self.throttle().await;
    }
}

fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let raw = resp.headers().get(reqwest::header::RETRY_AFTER)?;
    let secs: f64 = raw.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs_f64((secs + 0.5).min(60.0)))
}
