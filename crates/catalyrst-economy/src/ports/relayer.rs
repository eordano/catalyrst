use std::time::Duration;

use serde::Deserialize;

use crate::config::Config;
use crate::http::errors::ApiError;
use crate::ports::transaction::TransactionData;

const CANCEL_REQUEST_TIMEOUT_MS: u64 = 5000;

const BROADCAST_STATUSES: [&str; 3] = ["submitted", "mined", "confirmed"];
const FAILED_STATUSES: [&str; 3] = ["canceled", "failed", "expired"];

#[derive(Debug, Deserialize)]
struct OzResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<OzTransactionData>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OzTransactionData {
    id: String,
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    status_reason: Option<String>,
}

pub struct Relayer {
    http: reqwest::Client,
    base_url: String,
    relayer_id: String,
    api_key: String,
    speed: String,
    max_status_checks: u32,
    sleep: Duration,
}

impl Relayer {
    pub fn from_config(cfg: &Config) -> Option<Self> {
        if !cfg.has_relayer() {
            return None;
        }
        Some(Self {
            http: reqwest::Client::new(),
            base_url: cfg.relayer_url.clone().unwrap().trim_end_matches('/').to_string(),
            relayer_id: cfg.relayer_id.clone().unwrap(),
            api_key: cfg.relayer_api_key.clone().unwrap(),
            speed: cfg.relayer_speed.clone(),
            max_status_checks: cfg.relayer_max_status_checks,
            sleep: Duration::from_millis(cfg.relayer_sleep_ms),
        })
    }

    fn transactions_url(&self) -> String {
        format!(
            "{}/api/v1/relayers/{}/transactions",
            self.base_url, self.relayer_id
        )
    }

    fn transaction_url(&self, tx_id: &str) -> String {
        format!("{}/{}", self.transactions_url(), tx_id)
    }

    pub async fn send_meta_transaction(&self, tx: &TransactionData) -> Result<String, ApiError> {
        let to = &tx.params[0];
        let data = &tx.params[1];

        let resp = self
            .http
            .post(self.transactions_url())
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "to": to,
                "data": data,
                "speed": self.speed,
                "value": "0x0",
            }))
            .send()
            .await
            .map_err(|e| ApiError::RelayerFailed(format!("relayer request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            if status.as_u16() == 422 || status.as_u16() == 400 {
                return Err(ApiError::RelayReverted(body));
            }
            return Err(ApiError::RelayerFailed(format!(
                "The relayer responded with a {} status code: {}",
                status.as_u16(),
                body
            )));
        }

        let parsed: OzResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::RelayerFailed(format!("relayer decode failed: {e}")))?;

        let tx_data = match (parsed.success, parsed.data) {
            (true, Some(d)) => d,
            _ => {
                return Err(ApiError::RelayerFailed(
                    parsed
                        .error
                        .unwrap_or_else(|| "Unexpected response from relayer".into()),
                ));
            }
        };

        if let Some(hash) = &tx_data.hash {
            if BROADCAST_STATUSES.contains(&tx_data.status.as_str()) {
                return Ok(hash.clone());
            }
        }

        self.wait_for_broadcast(&tx_data.id).await
    }

    async fn wait_for_broadcast(&self, tx_id: &str) -> Result<String, ApiError> {
        let url = self.transaction_url(tx_id);

        for _ in 0..self.max_status_checks {
            tokio::time::sleep(self.sleep).await;

            let resp = match self
                .http
                .get(&url)
                .bearer_auth(&self.api_key)
                .send()
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };
            if !resp.status().is_success() {
                continue;
            }

            let parsed: OzResponse = match resp.json().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let Some(data) = parsed.data else {
                continue;
            };

            if FAILED_STATUSES.contains(&data.status.as_str()) {
                let reason = data
                    .status_reason
                    .clone()
                    .unwrap_or_else(|| data.status.clone());
                return Err(ApiError::RelayReverted(format!(
                    "Transaction {}: {}",
                    data.status, reason
                )));
            }

            if BROADCAST_STATUSES.contains(&data.status.as_str()) {
                if let Some(hash) = data.hash {
                    return Ok(hash);
                }
            }
        }

        self.cancel_transaction(tx_id).await;
        Err(ApiError::RelayerTimeout(
            "The relayer took too long to respond: The limit of status checks was reached".into(),
        ))
    }

    async fn cancel_transaction(&self, tx_id: &str) {
        let url = self.transaction_url(tx_id);
        let result = self
            .http
            .delete(&url)
            .bearer_auth(&self.api_key)
            .timeout(Duration::from_millis(CANCEL_REQUEST_TIMEOUT_MS))
            .send()
            .await;
        if let Err(e) = result {
            tracing::warn!(tx_id, error = %e, "relayer cancel request failed");
        }
    }
}
