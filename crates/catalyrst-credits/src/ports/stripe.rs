use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::http::ApiError;

type HmacSha256 = Hmac<Sha256>;

pub const DEFAULT_STRIPE_API_BASE: &str = "https://api.stripe.com";

pub const SIGNATURE_TOLERANCE_SECS: i64 = 300;

#[derive(Debug, Clone, Deserialize)]
pub struct PaymentIntent {
    pub id: String,
    pub client_secret: String,
}

#[derive(Clone)]
pub struct StripeClient {
    secret: String,
    base_url: String,
    client: reqwest::Client,
}

impl StripeClient {
    pub fn new(secret: String, base_url: String, client: reqwest::Client) -> Self {
        Self {
            secret,
            base_url,
            client,
        }
    }

    pub async fn create_payment_intent(
        &self,
        amount_cents: i64,
        currency: &str,
        address: &str,
        sku: &str,
        credits: &str,
        idempotency_key: &str,
    ) -> Result<PaymentIntent, ApiError> {
        let amount = amount_cents.to_string();
        let params = [
            ("amount", amount.as_str()),
            ("currency", currency),
            ("metadata[address]", address),
            ("metadata[sku]", sku),
            ("metadata[credits]", credits),
            ("automatic_payment_methods[enabled]", "true"),
        ];

        let url = format!("{}/v1/payment_intents", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.secret)
            .header("Idempotency-Key", idempotency_key)
            .form(&params)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("stripe request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();

            let body = resp.text().await.unwrap_or_default();
            tracing::error!(status, body = %truncate(&body, 500), "stripe payment_intent create failed");
            return Err(ApiError::Internal(format!(
                "stripe returned status {status}"
            )));
        }

        let pi: PaymentIntent = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("stripe response parse failed: {e}")))?;
        Ok(pi)
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

pub fn verify_stripe_signature(
    secret: &str,
    sig_header: &str,
    raw_body: &[u8],
    tolerance_secs: i64,
    now_unix: i64,
) -> bool {
    let mut timestamp: Option<i64> = None;
    let mut v1_sigs: Vec<&str> = Vec::new();
    for part in sig_header.split(',') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "t" => timestamp = value.parse::<i64>().ok(),
            "v1" => v1_sigs.push(value),
            _ => {}
        }
    }

    let Some(t) = timestamp else {
        return false;
    };
    if v1_sigs.is_empty() {
        return false;
    }

    if (now_unix as i128 - t as i128).abs() > tolerance_secs as i128 {
        return false;
    }

    let mut signed_payload = Vec::with_capacity(raw_body.len() + 16);
    signed_payload.extend_from_slice(t.to_string().as_bytes());
    signed_payload.push(b'.');
    signed_payload.extend_from_slice(raw_body);

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(&signed_payload);
    let expected = hex::encode(mac.finalize().into_bytes());

    v1_sigs
        .iter()
        .any(|cand| constant_time_eq(expected.as_bytes(), cand.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test_secret_abc123";

    fn sign(secret: &str, t: i64, body: &[u8]) -> String {
        let mut signed = Vec::new();
        signed.extend_from_slice(t.to_string().as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(body);
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(&signed);
        let sig = hex::encode(mac.finalize().into_bytes());
        format!("t={t},v1={sig}")
    }

    #[test]
    fn valid_signature_accepted() {
        let body = br#"{"id":"evt_1","type":"payment_intent.succeeded"}"#;
        let now = 1_690_000_000;
        let header = sign(SECRET, now, body);
        assert!(verify_stripe_signature(SECRET, &header, body, 300, now));
    }

    #[test]
    fn tampered_body_rejected() {
        let body = br#"{"id":"evt_1","type":"payment_intent.succeeded"}"#;
        let now = 1_690_000_000;
        let header = sign(SECRET, now, body);
        let tampered = br#"{"id":"evt_1","type":"payment_intent.SUCCEEDED!"}"#;
        assert!(!verify_stripe_signature(
            SECRET, &header, tampered, 300, now
        ));
    }

    #[test]
    fn wrong_secret_rejected() {
        let body = br#"{"id":"evt_1"}"#;
        let now = 1_690_000_000;
        let header = sign(SECRET, now, body);
        assert!(!verify_stripe_signature(
            "whsec_wrong_secret",
            &header,
            body,
            300,
            now
        ));
    }

    #[test]
    fn stale_timestamp_rejected() {
        let body = br#"{"id":"evt_1"}"#;
        let signed_at = 1_690_000_000;
        let header = sign(SECRET, signed_at, body);

        let now = signed_at + 301;
        assert!(!verify_stripe_signature(SECRET, &header, body, 300, now));

        assert!(!verify_stripe_signature(
            SECRET,
            &header,
            body,
            300,
            signed_at - 301
        ));

        assert!(verify_stripe_signature(
            SECRET,
            &header,
            body,
            300,
            signed_at + 300
        ));
    }

    #[test]
    fn multiple_v1_with_one_matching_accepted() {
        let body = br#"{"id":"evt_1"}"#;
        let now = 1_690_000_000;

        let valid = sign(SECRET, now, body);
        let valid_sig = valid.rsplit("v1=").next().unwrap();
        let header = format!(
            "t={now},v1=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef,v1={valid_sig},v0=ignored"
        );
        assert!(verify_stripe_signature(SECRET, &header, body, 300, now));
    }

    #[test]
    fn extreme_timestamp_does_not_panic() {
        let body = br#"{"id":"evt_1"}"#;
        let now = 1_690_000_000;
        assert!(!verify_stripe_signature(
            SECRET,
            &format!("t={},v1=abc", i64::MIN),
            body,
            300,
            now
        ));
        assert!(!verify_stripe_signature(
            SECRET,
            &format!("t={},v1=abc", i64::MAX),
            body,
            300,
            now
        ));
    }

    #[test]
    fn malformed_header_rejected() {
        let body = br#"{"id":"evt_1"}"#;
        let now = 1_690_000_000;

        assert!(!verify_stripe_signature(
            SECRET,
            "v1=abc123",
            body,
            300,
            now
        ));

        assert!(!verify_stripe_signature(
            SECRET,
            &format!("t={now}"),
            body,
            300,
            now
        ));

        assert!(!verify_stripe_signature(SECRET, "garbage", body, 300, now));

        assert!(!verify_stripe_signature(SECRET, "", body, 300, now));

        assert!(!verify_stripe_signature(
            SECRET,
            "t=notanumber,v1=abc",
            body,
            300,
            now
        ));
    }
}
