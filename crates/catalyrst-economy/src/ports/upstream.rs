use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;

use crate::config::Config;
use crate::http::errors::ApiError;

pub const DEFAULT_UPSTREAM_TIMEOUT_MS: u64 = 30_000;

/// Broadcast provider that forwards the already-validated request body verbatim
/// to a full transactions-server deployment and relays its response (status +
/// body) untouched, so a client talking to this node sees exactly what it would
/// see talking to the upstream.
pub struct UpstreamForwarder {
    http: reqwest::Client,
    url: String,
}

impl UpstreamForwarder {
    pub fn from_config(cfg: &Config) -> Option<Self> {
        let base = cfg.transactions_upstream_url.as_deref()?;
        Some(Self::new(
            base,
            Duration::from_millis(cfg.transactions_upstream_timeout_ms),
        ))
    }

    pub fn new(base_url: &str, timeout: Duration) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("reqwest client with a timeout always builds"),
            url: endpoint_url(base_url),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Sends the body to the upstream `/v1/transactions` endpoint. The outgoing
    /// request carries only the payload and its content-type: no header from
    /// the inbound request is copied, so node-local credentials (authorization,
    /// cookies, admin tokens) can never leak upstream.
    ///
    /// Transport failures split on whether the request can have reached the
    /// upstream. A connect-level failure (connection refused, DNS) provably
    /// never sent the body: it maps to `RelayerUnavailable` (503) and the
    /// quota slot is refunded. A timeout or response-read failure happens
    /// after the request went out, so the upstream may have broadcast the
    /// transaction anyway: it maps to `RelayerTimeout` (504), which
    /// `reservation_disposition` classifies Keep so a landed transaction can
    /// never escape the daily quota.
    pub async fn forward(
        &self,
        body: &[u8],
        content_type: Option<&str>,
    ) -> Result<ForwardedResponse, ApiError> {
        let resp = self
            .http
            .post(&self.url)
            .header(
                header::CONTENT_TYPE,
                content_type.unwrap_or("application/json"),
            )
            .body(body.to_vec())
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    ApiError::RelayerUnavailable(format!(
                        "The upstream transactions server could not be reached: {e}"
                    ))
                } else {
                    ApiError::RelayerTimeout(format!(
                        "The upstream transactions server did not answer after the \
                         request was sent, so the broadcast outcome is unknown: {e}"
                    ))
                }
            })?;

        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let body = resp.bytes().await.map_err(|e| {
            ApiError::RelayerTimeout(format!(
                "The upstream transactions server accepted the request but its \
                 response could not be read, so the broadcast outcome is unknown: {e}"
            ))
        })?;

        Ok(ForwardedResponse {
            status,
            content_type,
            body,
        })
    }
}

fn endpoint_url(base_url: &str) -> String {
    format!("{}/v1/transactions", base_url.trim_end_matches('/'))
}

/// An upstream response captured for verbatim relay to the client.
pub struct ForwardedResponse {
    status: u16,
    content_type: Option<String>,
    body: Bytes,
}

impl ForwardedResponse {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The upstream success payload is `{"ok":true,"txHash":"0x..."}`.
    pub fn tx_hash(&self) -> Option<String> {
        serde_json::from_slice::<serde_json::Value>(&self.body)
            .ok()?
            .get("txHash")?
            .as_str()
            .map(|s| s.to_string())
    }

    pub fn into_response(self) -> Response {
        let mut resp = Response::new(Body::from(self.body));
        *resp.status_mut() = StatusCode::from_u16(self.status).unwrap_or(StatusCode::BAD_GATEWAY);
        if let Some(ct) = self
            .content_type
            .as_deref()
            .and_then(|c| HeaderValue::from_str(c).ok())
        {
            resp.headers_mut().insert(header::CONTENT_TYPE, ct);
        }
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_is_the_upstream_v1_transactions_route() {
        assert_eq!(
            endpoint_url("https://transactions-api.decentraland.org"),
            "https://transactions-api.decentraland.org/v1/transactions"
        );
        assert_eq!(
            endpoint_url("https://transactions-api.decentraland.org/"),
            "https://transactions-api.decentraland.org/v1/transactions"
        );
    }

    #[test]
    fn tx_hash_reads_the_upstream_success_payload() {
        let resp = ForwardedResponse {
            status: 200,
            content_type: Some("application/json".into()),
            body: Bytes::from_static(br#"{"ok":true,"txHash":"0xabc"}"#),
        };
        assert_eq!(resp.tx_hash().as_deref(), Some("0xabc"));

        let no_hash = ForwardedResponse {
            status: 200,
            content_type: None,
            body: Bytes::from_static(br#"{"ok":true}"#),
        };
        assert_eq!(no_hash.tx_hash(), None);

        let not_json = ForwardedResponse {
            status: 200,
            content_type: None,
            body: Bytes::from_static(b"not json"),
        };
        assert_eq!(not_json.tx_hash(), None);
    }
}
