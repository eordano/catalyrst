use crate::http::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManaPaymentVerification {
    Pending,
    Reverted,
    NoPayment,
    Confirmed {
        from: String,
        to: String,
        value_wei: String,
    },
}

pub fn parse_verification(v: &serde_json::Value) -> Result<ManaPaymentVerification, ApiError> {
    match v.get("status").and_then(|s| s.as_str()) {
        Some("pending") => Ok(ManaPaymentVerification::Pending),
        Some("reverted") => Ok(ManaPaymentVerification::Reverted),
        Some("no_payment") => Ok(ManaPaymentVerification::NoPayment),
        Some("confirmed") => {
            let field = |name: &str| {
                v.get(name)
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_lowercase())
                    .ok_or_else(|| {
                        ApiError::Internal(format!(
                            "economy verify 'confirmed' response missing {name}"
                        ))
                    })
            };
            Ok(ManaPaymentVerification::Confirmed {
                from: field("from")?,
                to: field("to")?,
                value_wei: field("valueWei")?,
            })
        }
        other => Err(ApiError::Internal(format!(
            "economy verify returned unknown status {other:?}"
        ))),
    }
}

pub async fn verify_mana_payment(
    http: &reqwest::Client,
    economy_base_url: &str,
    economy_admin_token: &str,
    tx_hash: &str,
) -> Result<ManaPaymentVerification, ApiError> {
    let url = format!("{economy_base_url}/v1/payments/verify");
    let resp = http
        .post(&url)
        .bearer_auth(economy_admin_token)
        .json(&serde_json::json!({ "txHash": tx_hash }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("economy verify request failed: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        let txt = resp.text().await.unwrap_or_default();
        if code == 400 {
            return Err(ApiError::bad_request(format!(
                "economy rejected the txHash: {}",
                truncate(&txt, 300)
            )));
        }
        return Err(ApiError::Internal(format!(
            "economy verify status {code}: {}",
            truncate(&txt, 300)
        )));
    }

    let parsed: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("economy verify parse failed: {e}")))?;
    parse_verification(&parsed)
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_every_documented_status() {
        assert_eq!(
            parse_verification(&json!({"status": "pending"})).unwrap(),
            ManaPaymentVerification::Pending
        );
        assert_eq!(
            parse_verification(&json!({"status": "reverted"})).unwrap(),
            ManaPaymentVerification::Reverted
        );
        assert_eq!(
            parse_verification(&json!({"status": "no_payment", "valueWei": "0"})).unwrap(),
            ManaPaymentVerification::NoPayment
        );
        assert_eq!(
            parse_verification(&json!({
                "status": "confirmed",
                "from": "0xAAAA567890123456789012345678901234567890",
                "to": "0xbbbb567890123456789012345678901234567890",
                "valueWei": "2500000000000000000",
            }))
            .unwrap(),
            ManaPaymentVerification::Confirmed {
                from: "0xaaaa567890123456789012345678901234567890".into(),
                to: "0xbbbb567890123456789012345678901234567890".into(),
                value_wei: "2500000000000000000".into(),
            }
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_shapes() {
        assert!(parse_verification(&json!({"status": "???"})).is_err());
        assert!(parse_verification(&json!({})).is_err());
        assert!(parse_verification(&json!({"status": "confirmed"})).is_err());
        assert!(
            parse_verification(&json!({"status": "confirmed", "from": "0x1", "to": "0x2"}))
                .is_err()
        );
    }
}
