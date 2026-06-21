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

pub async fn post_newsletter(
    State(state): State<AppState>,
    body: Option<Json<NewsletterBody>>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let email = body.email.unwrap_or_default().trim().to_string();
    let source = body.source.unwrap_or_else(|| "Builder".to_string());

    if !email.is_empty() {
        if let Err(e) = state.newsletter.subscribe(&email, &source).await {
            tracing::warn!(error = %e, "newsletter local archive write failed (ignored)");
        }
    }

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
