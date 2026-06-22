use serde::Deserialize;

use crate::http::ApiError;

/// External captcha provider verifier (hCaptcha / reCAPTCHA siteverify).
/// Both providers share the same contract: an `application/x-www-form-urlencoded`
/// POST of `secret` + `response` (the client token) + optional `remoteip`,
/// answered with a JSON object carrying a top-level `success` boolean. Kept
/// provider-agnostic so the operator only supplies a secret and (optionally) a
/// different verify URL.
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

/// Parse a provider siteverify reply into a pass/fail verdict. Anything that is
/// not a well-formed body with `success: true` fails closed — a captcha gate that
/// passed on a malformed or error response would be no gate at all.
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

    /// Verify a client-supplied token against the provider. Returns `Ok(true)`
    /// only on a `success: true` reply; network or HTTP failures surface as an
    /// error so the claim aborts rather than silently granting credits.
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
        assert!(!parse_verdict(r#"{"success":false,"error-codes":["invalid-input-response"]}"#));
        assert!(!parse_verdict(r#"{}"#));
        assert!(!parse_verdict("not json"));
        assert!(!parse_verdict(""));
    }
}
