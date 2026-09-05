use serde::Deserialize;

use crate::http::ApiError;

#[derive(Clone)]
pub struct CaptchaProvider {
    secret: String,
    verify_url: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SiteVerifyResponse {
    #[serde(default)]
    success: bool,
}

pub fn parse_verdict(body: &str) -> bool {
    serde_json::from_str::<SiteVerifyResponse>(body)
        .map(|r| r.success)
        .unwrap_or(false)
}

impl CaptchaProvider {
    pub fn new(secret: String, verify_url: String, client: reqwest::Client) -> Self {
        Self {
            secret,
            verify_url,
            client,
        }
    }

    pub async fn verify(&self, token: &str, remoteip: Option<&str>) -> Result<bool, ApiError> {
        let mut form = vec![("secret", self.secret.as_str()), ("response", token)];
        if let Some(ip) = remoteip {
            form.push(("remoteip", ip));
        }

        let resp = self
            .client
            .post(&self.verify_url)
            .form(&form)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("captcha provider request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ApiError::Internal(format!(
                "captcha provider returned status {}",
                resp.status().as_u16()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ApiError::Internal(format!("captcha provider read failed: {e}")))?;

        Ok(parse_verdict(&body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_true_on_success() {
        assert!(parse_verdict(r#"{"success":true}"#));
        assert!(parse_verdict(
            r#"{"success":true,"challenge_ts":"2026-06-21T00:00:00Z","hostname":"x"}"#
        ));
    }

    #[test]
    fn verdict_false_on_failure_or_garbage() {
        assert!(!parse_verdict(r#"{"success":false}"#));
        assert!(!parse_verdict(
            r#"{"success":false,"error-codes":["invalid-input-response"]}"#
        ));
        assert!(!parse_verdict(r#"{}"#));
        assert!(!parse_verdict("not json"));
        assert!(!parse_verdict(""));
    }
}
