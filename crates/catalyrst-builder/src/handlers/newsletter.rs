use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::errors::ApiError;
use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct NewsletterBody {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

pub fn is_valid_email(email: &str) -> bool {
    if email.len() > 254 {
        return false;
    }
    if email.chars().any(|c| {
        c.is_whitespace() || c.is_control() || c == ',' || c == ';' || c == '<' || c == '>'
    }) {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    if local.is_empty() || local.len() > 64 || domain.contains('@') {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

pub async fn post_newsletter(
    State(state): State<AppState>,
    body: Option<Json<NewsletterBody>>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let email = body.email.unwrap_or_default().trim().to_ascii_lowercase();
    let source = body.source.unwrap_or_else(|| "Builder".to_string());

    if email.is_empty() {
        return Err(ApiError::bad_request("email is required"));
    }
    if !is_valid_email(&email) {
        return Err(ApiError::bad_request("invalid email address"));
    }

    state.newsletter.subscribe(&email, &source).await?;

    if let (Some(url), Some(publication_id), Some(api_key)) = (
        &state.newsletter_service_url,
        &state.newsletter_publication_id,
        &state.newsletter_api_key,
    ) {
        let target = format!(
            "{}/publications/{}/subscriptions",
            url.trim_end_matches('/'),
            publication_id
        );
        let req = state
            .http
            .post(&target)
            .bearer_auth(api_key)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&json!({
                "email": email,
                "reactivate_existing": true,
                "send_welcome_email": false,
                "utm_source": source,
                "utm_medium": "organic",
            }));
        if let Err(e) = req.send().await {
            tracing::warn!(error = %e, "newsletter SaaS forward failed (ignored)");
        }
    }

    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::is_valid_email;

    #[test]
    fn accepts_normal_addresses() {
        for good in [
            "a@b.co",
            "user@example.com",
            "first.last+tag@sub.example.org",
            "UPPER@EXAMPLE.COM",
            "x_y-z@ex-ample.io",
        ] {
            assert!(is_valid_email(good), "expected valid: {good}");
        }
    }

    #[test]
    fn rejects_malformed_addresses() {
        for bad in [
            "",
            "plainaddress",
            "@example.com",
            "user@",
            "user@localhost",
            "user@@example.com",
            "user@.com",
            "user@example.",
            "user@-example.com",
            "user name@example.com",
            "user@exam ple.com",
            "user@example.com\n",
            "<user@example.com>",
            "a@b,c.com",
        ] {
            assert!(!is_valid_email(bad), "expected invalid: {bad}");
        }
    }

    #[test]
    fn rejects_oversized_addresses() {
        let long_local = format!("{}@example.com", "a".repeat(65));
        assert!(!is_valid_email(&long_local));
        let long_total = format!("a@{}.com", "b".repeat(260));
        assert!(!is_valid_email(&long_total));
    }
}
